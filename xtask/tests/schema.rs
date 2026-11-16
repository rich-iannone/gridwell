//! The IR's JSON Schema says what gridwell reads. `crates/gridwell-ir/schema/` holds
//! one schema per IR version, and `docs/schema/` serves a copy of each at its `$id`.
//! The current one, `gridwell_ir::JSON_SCHEMA`:
//!
//! - fits every table gridwell accepts, as gridwell writes it back, and every table
//!   that fits it parses, except a count written as `1.0` (an integer to JSON Schema);
//! - lists every field of every object, with gridwell's default, requires the fields
//!   gridwell requires, and lists each keyword's canonical values;
//! - rejects what gridwell rejects in a field by itself (a wrong type, an unknown
//!   keyword, a malformed length, link or language tag, a typed value not in its
//!   type's form), except color names and the numbers in a color function, whose
//!   form is all it checks.
//!
//! And no schema accepts less than an earlier one of its major version, so a table
//! that fits IR 1.0's schema fits every 1.x schema.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use gridwell_ir::{
    BorderStyle, CellScope, FontStyle, FontWeight, HAlign, IrVersion, Keyword, Overflow,
    PageBreakMode, RowParity, RowRole, SelectorScope, SummaryPlacement, Table, TextDecoration,
    TextDirection, TextOverflow, TextTransform, VAlign, ValidationRule, ValueType, WhiteSpace,
    WordBreak, IR_VERSION, JSON_SCHEMA,
};
use jsonschema::{PatternOptions, Validator};
use serde_json::{json, Value};

const SCHEMA_DIR: &str = "crates/gridwell-ir/schema";
const DOCS_DIR: &str = "docs/schema";
const ID_BASE: &str = "https://rich-iannone.github.io/gridwell/schema/";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// The schema files, oldest first: each one's version and file name.
fn schema_files() -> Vec<(IrVersion, String)> {
    let mut out: Vec<(IrVersion, String)> = std::fs::read_dir(root().join(SCHEMA_DIR))
        .unwrap()
        .map(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            let version = name
                .strip_prefix("ir-")
                .and_then(|n| n.strip_suffix(".schema.json"))
                .and_then(IrVersion::parse)
                .unwrap_or_else(|| panic!("{SCHEMA_DIR}/{name}: not ir-MAJOR.MINOR.schema.json"));
            (version, name)
        })
        .collect();
    out.sort();
    out
}

fn compile(schema: &Value) -> Validator {
    jsonschema::draft202012::options()
        .build(schema)
        .unwrap_or_else(|e| panic!("the schema doesn't compile: {e}"))
}

/// The same with the `regex` engine, which has no backtracking: the patterns mean the
/// same to both.
fn compile_with_regex(schema: &Value) -> Validator {
    jsonschema::draft202012::options()
        .with_pattern_options(PatternOptions::regex())
        .build(schema)
        .unwrap_or_else(|e| panic!("the schema doesn't compile with `regex`: {e}"))
}

fn schema() -> &'static Value {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    SCHEMA.get_or_init(|| serde_json::from_str(JSON_SCHEMA).unwrap())
}

fn validators() -> &'static (Validator, Validator) {
    static VALIDATORS: OnceLock<(Validator, Validator)> = OnceLock::new();
    VALIDATORS.get_or_init(|| (compile(schema()), compile_with_regex(schema())))
}

/// Whether `v` fits the current schema (by both regex engines, which must agree).
fn fits(v: &Value) -> bool {
    let (fancy, plain) = validators();
    let fits = fancy.is_valid(v);
    assert_eq!(fits, plain.is_valid(v), "the regex engines disagree on {v}");
    fits
}

/// Why `v` doesn't fit the current schema.
fn misfit(v: &Value) -> String {
    let errors: Vec<String> = validators()
        .0
        .iter_errors(v)
        .take(3)
        .map(|e| format!("{}: {e}", e.instance_path()))
        .collect();
    errors.join("; ")
}

/// What gridwell makes of a table.
#[derive(Debug)]
enum Verdict {
    /// It doesn't parse.
    Unparsed,
    /// It parses, with these validation errors.
    Invalid(Vec<ValidationRule>),
    /// It is valid: the table as gridwell writes it back.
    Accepted(Value),
}

fn verdict(v: &Value) -> Verdict {
    match Table::from_json(&v.to_string()) {
        Err(_) => Verdict::Unparsed,
        Ok(table) => {
            let errors = table.validate();
            if errors.is_empty() {
                Verdict::Accepted(serde_json::to_value(&table).unwrap())
            } else {
                Verdict::Invalid(errors.iter().map(|e| e.rule).collect())
            }
        }
    }
}

/// `v` written back by gridwell, if it parses.
fn written_back(v: &Value) -> Option<Value> {
    let table = Table::from_json(&v.to_string()).ok()?;
    Some(serde_json::to_value(&table).unwrap())
}

/// Validation rules about a field by itself, not about the rest of the table. The
/// schema checks each of them, but `INVALID_COLOR`, as it checks only a color's form.
const FIELD_RULES: &[ValidationRule] = &[
    ValidationRule::IrVersion,
    ValidationRule::UnknownValue,
    ValidationRule::InvalidLength,
    ValidationRule::InvalidStyleId,
    ValidationRule::InvalidLang,
    ValidationRule::InvalidHref,
    ValidationRule::InvalidTableId,
    ValidationRule::InvalidTypedValue,
    ValidationRule::SpanZeroValue,
];

/// Runs `check` on every item, on every core, and gathers what it finds, in order.
fn par<T: Sync, R: Send>(items: &[T], check: impl Fn(&T) -> Vec<R> + Sync) -> Vec<R> {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let chunk = items.len().div_ceil(cores).max(1);
    let check = &check;
    std::thread::scope(|s| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|c| s.spawn(move || c.iter().flat_map(check).collect::<Vec<R>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    })
}

// --- The table's objects and fields ----------------------------------------------

/// `maximal.json` (every field of every object), as gridwell writes it back, with a
/// node of a later version's type, with a fallback, in its first cell.
fn maximal() -> Value {
    static MAXIMAL: OnceLock<Value> = OnceLock::new();
    MAXIMAL
        .get_or_init(|| {
            let mut v: Value =
                serde_json::from_str(&read("xtask/tests/docs/maximal.json")).unwrap();
            v.pointer_mut("/table/tbody/0/rows/0/cells/0/content")
                .unwrap()
                .as_array_mut()
                .unwrap()
                .push(json!({
                    "type": "later_node",
                    "value": "not drawn",
                    "fallback": [{"type": "text", "value": "drawn instead", "href": "https://example.com/"}]
                }));
            match verdict(&v) {
                Verdict::Accepted(back) => back,
                other => panic!("maximal.json isn't valid: {other:?}"),
            }
        })
        .clone()
}

fn ref_name(s: &Value) -> Option<&str> {
    s.get("$ref")?.as_str()?.strip_prefix("#/$defs/")
}

/// A definition of the current schema; `#` is the schema itself (the table).
fn definition(name: &str) -> &'static Value {
    if name == "#" {
        return schema();
    }
    schema()["$defs"]
        .get(name)
        .unwrap_or_else(|| panic!("the schema has no definition `{name}`"))
}

/// Every object of a table, with the definition the schema checks it against:
/// (JSON pointer, definition name).
fn objects(table: &Value) -> Vec<(String, String)> {
    let mut out = vec![(String::new(), "#".to_string())];
    walk(schema(), table, "", &mut out);
    out
}

fn walk(sub: &Value, v: &Value, pointer: &str, out: &mut Vec<(String, String)>) {
    if let Some(name) = ref_name(sub) {
        let def = definition(name);
        if v.is_object() && def["type"] == "object" {
            out.push((pointer.to_string(), name.to_string()));
        }
        return walk(def, v, pointer, out);
    }
    // An `anyOf` of definitions: a content node's branch is its type's, and another
    // has one object branch. (An `anyOf` of constraints, as a typed value's, is
    // beside its object's properties.)
    let names: Vec<&str> = sub["anyOf"]
        .as_array()
        .map_or(Vec::new(), |b| b.iter().filter_map(ref_name).collect());
    if let (false, Value::Object(map)) = (names.is_empty(), v) {
        let name = if names.iter().all(|n| n.ends_with("_node")) {
            let own = format!(
                "{}_node",
                map.get("type").and_then(Value::as_str).unwrap_or("")
            );
            if names.contains(&own.as_str()) {
                own
            } else {
                "unknown_node".to_string()
            }
        } else {
            let name = names.iter().find(|n| definition(n)["type"] == "object");
            name.unwrap_or_else(|| panic!("no object branch at {pointer}"))
                .to_string()
        };
        return walk(
            &json!({ "$ref": format!("#/$defs/{name}") }),
            v,
            pointer,
            out,
        );
    }
    match v {
        Value::Object(map) => {
            for (k, item) in map {
                let p = format!("{pointer}/{}", k.replace('~', "~0").replace('/', "~1"));
                if let Some(s) = sub["properties"].get(k) {
                    walk(s, item, &p, out);
                } else if sub["additionalProperties"].is_object() {
                    walk(&sub["additionalProperties"], item, &p, out);
                }
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                walk(&sub["items"], item, &format!("{pointer}/{i}"), out);
            }
        }
        _ => {}
    }
}

/// A field of `maximal()`: its object's pointer and definition, and its name.
#[derive(Clone, Debug)]
struct Field {
    pointer: String,
    def: String,
    key: String,
}

impl Field {
    fn name(&self) -> String {
        format!("{}.{}", self.def, self.key)
    }

    /// The field's schema (`None` for a field of an unknown node).
    fn schema(&self) -> Option<&'static Value> {
        definition(&self.def)["properties"].get(&self.key)
    }

    /// `maximal()` with the field set to `value`, or removed.
    fn with(&self, value: Option<Value>) -> Value {
        let mut v = maximal();
        let object = v
            .pointer_mut(&self.pointer)
            .unwrap()
            .as_object_mut()
            .unwrap();
        match value {
            Some(value) => object.insert(self.key.clone(), value),
            None => object.remove(&self.key),
        };
        v
    }

    /// The field's value in a table.
    fn of(&self, table: &Value) -> Value {
        table
            .pointer(&format!("{}/{}", self.pointer, self.key))
            .cloned()
            .unwrap_or(Value::Null)
    }
}

/// Each field of each definition, where `maximal()` first has it.
fn fields() -> Vec<Field> {
    let max = maximal();
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (pointer, def) in objects(&max) {
        for key in max.pointer(&pointer).unwrap().as_object().unwrap().keys() {
            if seen.insert((def.clone(), key.clone())) {
                out.push(Field {
                    pointer: pointer.clone(),
                    def: def.clone(),
                    key: key.clone(),
                });
            }
        }
    }
    out
}

/// The value definition a field's schema refers to (`size`, `color`, …), directly or
/// as the non-null branch of an `anyOf`.
fn value_def(field_schema: &Value) -> Option<&str> {
    let name = ref_name(field_schema).or_else(|| {
        let branches = field_schema["anyOf"].as_array()?;
        branches.iter().find_map(ref_name)
    })?;
    (definition(name)["type"] != "object").then_some(name)
}

/// A value with every `null` field of its objects left out, which an absent field
/// reads as.
fn without_nulls(v: &Value) -> Value {
    match v {
        Value::Object(o) => Value::Object(
            o.iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k.clone(), without_nulls(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(without_nulls).collect()),
        v => v.clone(),
    }
}

// --- The files -------------------------------------------------------------------

#[test]
fn the_schema_files_are_the_versions_and_the_docs_serve_each_one() {
    let files = schema_files();
    let (latest, name) = files.last().expect("no schema files");
    assert_eq!(
        format!("{}.{}", latest.major, latest.minor),
        IR_VERSION,
        "the newest schema isn't IR_VERSION's"
    );
    assert_eq!(read(&format!("{SCHEMA_DIR}/{name}")), JSON_SCHEMA);
    for (version, name) in &files {
        let text = read(&format!("{SCHEMA_DIR}/{name}"));
        assert_eq!(
            read(&format!("{DOCS_DIR}/{name}")),
            text,
            "{DOCS_DIR}/{name} isn't a copy of {SCHEMA_DIR}/{name}"
        );
        let schema: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert_eq!(schema["$id"], format!("{ID_BASE}{name}"));
        assert_eq!(
            schema["title"],
            format!("gridwell IR {}.{}", version.major, version.minor)
        );
        assert!(
            jsonschema::meta::is_valid(&schema),
            "{name}: not a valid draft 2020-12 schema"
        );
        compile(&schema);
        compile_with_regex(&schema);
        // `ir_version` fits a schema of its major version, as gridwell reads it.
        let pattern = compile(&schema["properties"]["ir_version"]);
        let m = version.major;
        for ok in [
            format!("{m}.0"),
            format!("{m}.7"),
            format!("0{m}.00"),
            format!("{m}.{}", version.minor + 1),
        ] {
            assert!(
                pattern.is_valid(&json!(ok)),
                "{name}: ir_version {ok:?} doesn't fit"
            );
        }
        for bad in [
            format!("{}.0", m + 1),
            format!("{m}"),
            format!("{m}.0.0"),
            format!(" {m}.0"),
            format!("{m}.x"),
        ] {
            assert!(
                !pattern.is_valid(&json!(bad)),
                "{name}: ir_version {bad:?} fits"
            );
        }
    }
    let docs: BTreeSet<String> = std::fs::read_dir(root().join(DOCS_DIR))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect();
    let names: BTreeSet<String> = files.into_iter().map(|(_, n)| n).collect();
    assert_eq!(
        docs, names,
        "{DOCS_DIR} serves other schemas than {SCHEMA_DIR} has"
    );
}

/// Every IR file in the repository (tracked, or untracked and not ignored, so a
/// checkout and a working copy find the same): a JSON object with an `ir_version`.
fn ir_files() -> Vec<(String, Value)> {
    let listed = std::process::Command::new("git")
        .arg("-C")
        .arg(root())
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
            "*.json",
        ])
        .output()
        .expect("git runs");
    assert!(listed.status.success(), "git ls-files failed");
    let mut out = Vec::new();
    for rel in listed.stdout.split(|&b| b == 0).filter(|s| !s.is_empty()) {
        let rel = String::from_utf8_lossy(rel).replace('\\', "/");
        if rel.ends_with(".grid.json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(root().join(&rel)) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if v.get("ir_version").is_some() {
            out.push((rel, v));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn every_table_gridwell_accepts_fits_the_schema_as_written_back() {
    let files = ir_files();
    let results = par(&files, |(rel, v)| {
        let mut out = Vec::new();
        let fits_raw = fits(v);
        match verdict(v) {
            Verdict::Accepted(back) => {
                if !fits(&back) {
                    out.push(Err(format!(
                        "{rel}: gridwell accepts it, but written back it doesn't fit: {}",
                        misfit(&back)
                    )));
                }
                out.push(Ok(("accepted", fits_raw)));
            }
            Verdict::Unparsed if fits_raw => {
                out.push(Err(format!("{rel}: fits the schema but doesn't parse")));
            }
            Verdict::Unparsed => out.push(Ok(("unparsed", false))),
            Verdict::Invalid(_) => out.push(Ok(("invalid", fits_raw))),
        }
        out
    });
    let problems: Vec<String> = results.iter().filter_map(|r| r.clone().err()).collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    let mut counts: BTreeMap<(&str, bool), usize> = BTreeMap::new();
    for (kind, fit) in results.iter().filter_map(|r| r.as_ref().ok()) {
        *counts.entry((*kind, *fit)).or_default() += 1;
    }
    eprintln!(
        "{} IR files, by verdict and whether they fit as written: {counts:?}",
        files.len()
    );
    // The fixtures, the docs, the conformance kit and compat/ (which holds the
    // example corpus): 140 at the freeze.
    let accepted = counts.get(&("accepted", true)).copied().unwrap_or(0)
        + counts.get(&("accepted", false)).copied().unwrap_or(0);
    assert!(accepted >= 140, "only {accepted} accepted IR files found");
}

/// Each invalid fixture breaks one rule (`_expect`): the schema refuses it when the
/// rule is about a field by itself, and leaves it to validation when the rule is about
/// the rest of the table.
#[test]
fn the_schema_refuses_the_invalid_fixtures_it_can_tell_by_their_fields() {
    let dir = root().join("fixtures/invalid");
    let mut problems = Vec::new();
    let mut counts = [0; 2];
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let expect: Vec<&str> = v["_expect"]
            .as_array()
            .unwrap_or_else(|| panic!("{name} has no _expect"))
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        if expect == ["INVALID_COLOR"] {
            continue; // the schema checks a color's form only
        }
        let by_field = expect
            .iter()
            .any(|e| FIELD_RULES.iter().any(|r| r.id() == *e));
        counts[by_field as usize] += 1;
        if fits(&v) == by_field {
            problems.push(format!(
                "{name} ({}): fits the schema: {}",
                expect.join(", "),
                fits(&v)
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(
        counts[0] > 10 && counts[1] > 5,
        "only {counts:?} fixtures by the rest of the table and by a field"
    );
}

#[test]
fn the_schema_has_every_field_of_every_object_and_requires_what_gridwell_requires() {
    let max = maximal();
    let mut problems = Vec::new();
    for (pointer, def) in objects(&max) {
        if def == "unknown_node" {
            continue;
        }
        let props: BTreeSet<&String> = definition(&def)["properties"]
            .as_object()
            .unwrap()
            .keys()
            .collect();
        let keys: BTreeSet<&String> = max
            .pointer(&pointer)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .collect();
        if props != keys {
            problems.push(format!(
                "{def} at {pointer}: gridwell writes {keys:?}, the schema lists {props:?}"
            ));
        }
    }
    let fields = fields();
    problems.extend(par(&fields, |f| {
        let mut out = Vec::new();
        let def = definition(&f.def);
        let required = def["required"]
            .as_array()
            .is_some_and(|r| r.contains(&json!(f.key)));
        let without = f.with(None);
        let back = written_back(&without);
        match (&back, required) {
            (None, false) => out.push(format!(
                "{}: gridwell requires it; the schema doesn't",
                f.name()
            )),
            (Some(_), true) => out.push(format!(
                "{}: the schema requires it; gridwell doesn't",
                f.name()
            )),
            _ => {}
        }
        if fits(&without) != back.is_some() {
            out.push(format!(
                "{}: left out, the table fits: {}, parses: {}",
                f.name(),
                fits(&without),
                back.is_some()
            ));
        }
        if let (Some(back), Some(field)) = (&back, f.schema()) {
            match field.get("default") {
                None => out.push(format!("{}: optional, but has no default", f.name())),
                Some(d) if without_nulls(d) != without_nulls(&f.of(back)) => out.push(format!(
                    "{}: the default is {d}, but gridwell reads it as {}",
                    f.name(),
                    f.of(back)
                )),
                Some(_) => {}
            }
        }
        out
    }));
    // Every object definition was checked.
    let seen: BTreeSet<String> = fields.iter().map(|f| f.def.clone()).collect();
    for (name, d) in schema()["$defs"].as_object().unwrap() {
        if d["type"] == "object" && !seen.contains(name) {
            problems.push(format!("maximal.json has no {name}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// A field gridwell doesn't know, as a later minor version may add, of any JSON kind on
/// any object: gridwell ignores it, so it fits.
#[test]
fn an_unknown_field_on_any_object_fits_as_gridwell_ignores_it() {
    let max = maximal();
    let objects = objects(&max);
    let values = [
        Value::Null,
        json!(true),
        json!(1),
        json!(1.5),
        json!("x"),
        json!([1]),
        json!({"type": "text"}),
    ];
    let problems = par(&objects, |(pointer, def)| {
        let mut out = Vec::new();
        for value in &values {
            let mut v = max.clone();
            let object = v.pointer_mut(pointer).unwrap().as_object_mut().unwrap();
            object.insert("later_field".to_string(), value.clone());
            if !matches!(verdict(&v), Verdict::Accepted(_)) {
                out.push(format!(
                    "{def} at {pointer}: gridwell doesn't ignore an unknown {value}"
                ));
            } else if !fits(&v) {
                out.push(format!(
                    "{def} at {pointer}: an unknown {value} doesn't fit: {}",
                    misfit(&v)
                ));
            }
        }
        out
    });
    let seen: BTreeSet<&str> = objects.iter().map(|(_, def)| def.as_str()).collect();
    assert!(seen.len() > 25, "only {} kinds of object", seen.len());
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Values of every JSON kind, put in every field.
fn probes() -> Vec<Value> {
    vec![
        Value::Null,
        json!(true),
        json!(false),
        json!(0),
        json!(1),
        json!(-1),
        json!(1.5),
        json!(4_294_967_296u64),
        json!(1e300),
        json!(""),
        json!(" "),
        json!("x"),
        json!("0"),
        json!("1"),
        json!("1px"),
        json!("-1px"),
        json!("auto"),
        json!("1fr"),
        json!("50%"),
        json!("red"),
        json!("bold"),
        json!("Center"),
        json!("en"),
        json!("https://example.com"),
        json!("text"),
        json!([]),
        json!([1]),
        json!([{"type": "text", "value": "x"}]),
        json!({}),
        json!({"type": "text", "value": "x"}),
    ]
}

enum Finding {
    Problem(String),
    /// A field where `1.0` fits the schema but doesn't parse.
    FloatInteger(String),
}

#[test]
fn a_value_of_any_kind_in_any_field_fits_as_gridwell_reads_it() {
    let fields = fields();
    let findings = par(&fields, |f| {
        let mut out = Vec::new();
        for probe in probes().into_iter().chain([json!(1.0), json!(100.0)]) {
            let v = f.with(Some(probe.clone()));
            let fit = fits(&v);
            let at = format!("{} = {probe}", f.name());
            match verdict(&v) {
                Verdict::Unparsed
                    if fit && probe.is_f64() && probe.as_f64().unwrap().fract() == 0.0 =>
                {
                    out.push(Finding::FloatInteger(f.name()));
                }
                Verdict::Unparsed if fit => out.push(Finding::Problem(format!(
                    "{at}: fits the schema but doesn't parse"
                ))),
                Verdict::Accepted(back) if !fits(&back) => out.push(Finding::Problem(format!(
                    "{at}: gridwell accepts it, but written back it doesn't fit: {}",
                    misfit(&back)
                ))),
                Verdict::Invalid(rules) if fit && rules.iter().all(|r| FIELD_RULES.contains(r)) => {
                    out.push(Finding::Problem(format!(
                        "{at}: fits the schema, but gridwell rejects it: {rules:?}"
                    )))
                }
                _ => {}
            }
        }
        out
    });
    let mut problems = Vec::new();
    let mut float_integers = BTreeSet::new();
    for finding in findings {
        match finding {
            Finding::Problem(p) => problems.push(p),
            Finding::FloatInteger(f) => {
                float_integers.insert(f);
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    // The one table that fits but doesn't parse: a count written with a fraction.
    // JSON Schema calls `1.0` an integer, and gridwell reads a count only from a whole
    // number (docs/schema/index.qmd says so).
    let counts: BTreeSet<String> = fields
        .iter()
        .filter(|f| {
            f.schema().is_some_and(|s| {
                s["type"] == "integer"
                    || s["type"]
                        .as_array()
                        .is_some_and(|t| t.contains(&json!("integer")))
            })
        })
        .map(Field::name)
        .collect();
    assert_eq!(float_integers, counts);
    assert!(counts.contains("cell.colspan") && counts.contains("config.table_cols"));
}

// --- Keywords --------------------------------------------------------------------

type KeywordValues = (
    &'static [&'static str],
    &'static [(&'static str, &'static str)],
);

fn kw<K: Keyword>() -> KeywordValues {
    (K::ALLOWED, K::ALIASES)
}

/// The keyword type of a field, by its definition in the schema.
fn keyword_of(def: &str, key: &str) -> Option<KeywordValues> {
    Some(match (def, key) {
        ("column", "align") | ("style_def", "text_align") => kw::<HAlign>(),
        ("style_def", "vertical_align") => kw::<VAlign>(),
        ("row", "role") => kw::<RowRole>(),
        ("row_group", "summary_placement") => kw::<SummaryPlacement>(),
        ("cell", "scope") => kw::<CellScope>(),
        ("config", "page_break_mode") => kw::<PageBreakMode>(),
        ("config", "dir") => kw::<TextDirection>(),
        ("border", "style") => kw::<BorderStyle>(),
        ("style_def", "font_style") => kw::<FontStyle>(),
        ("style_def", "text_transform") => kw::<TextTransform>(),
        ("style_def", "text_decoration") => kw::<TextDecoration>(),
        ("style_def", "white_space") => kw::<WhiteSpace>(),
        ("style_def", "word_break") => kw::<WordBreak>(),
        ("style_def", "overflow") | ("config", "container_overflow") => kw::<Overflow>(),
        ("style_def", "text_overflow") => kw::<TextOverflow>(),
        ("conditional_selector", "row_parity") => kw::<RowParity>(),
        ("conditional_selector", "scope") => kw::<SelectorScope>(),
        ("typed_value", "type") => kw::<ValueType>(),
        _ => return None,
    })
}

#[test]
fn every_keyword_lists_gridwells_values_and_other_spellings_fit_once_written_back() {
    let mut problems = Vec::new();
    let mut checked = 0;
    for f in fields() {
        let Some(enumerated) = f.schema().and_then(|s| s["enum"].as_array()) else {
            continue;
        };
        let Some((allowed, aliases)) = keyword_of(&f.def, &f.key) else {
            problems.push(format!("{}: a keyword this test doesn't know", f.name()));
            continue;
        };
        checked += 1;
        let listed: Vec<&str> = enumerated.iter().filter_map(Value::as_str).collect();
        if listed != allowed {
            problems.push(format!(
                "{}: the schema lists {listed:?}, gridwell {allowed:?}",
                f.name()
            ));
        }
        // gridwell reads a keyword in any case, trimmed, and its aliases; the schema
        // has only the canonical spelling, which gridwell writes back.
        let mut spellings: Vec<(String, &str)> =
            aliases.iter().map(|(a, c)| (a.to_string(), *c)).collect();
        for a in allowed {
            spellings.push((a.to_uppercase(), a));
            spellings.push((format!(" {a}\t"), a));
        }
        for (spelling, canonical) in spellings {
            let v = f.with(Some(json!(spelling)));
            if fits(&v) {
                problems.push(format!("{}: {spelling:?} fits", f.name()));
            }
            match verdict(&v) {
                Verdict::Unparsed => {
                    problems.push(format!("{}: {spelling:?} doesn't parse", f.name()))
                }
                Verdict::Invalid(rules) if rules.contains(&ValidationRule::UnknownValue) => {
                    problems.push(format!("{}: {spelling:?} is an unknown value", f.name()))
                }
                _ => {}
            }
            let back = written_back(&v).unwrap();
            if f.of(&back) != json!(canonical) {
                problems.push(format!(
                    "{}: {spelling:?} is written back as {}",
                    f.name(),
                    f.of(&back)
                ));
            }
        }
    }
    assert_eq!(checked, 20, "keyword fields checked");

    // A font weight is a keyword or a number, written back as a string.
    let weights = &definition("font_weight")["anyOf"][0]["enum"];
    let listed: Vec<&str> = weights
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let allowed: Vec<&str> = FontWeight::ALLOWED
        .iter()
        .copied()
        .filter(|a| *a != "1–1000")
        .collect();
    assert_eq!(listed, allowed);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

// --- Values ----------------------------------------------------------------------

/// Unicode's White_Space characters (what Rust's `trim` takes off), and some that
/// aren't.
const SPACES: &[&str] = &[
    " ", "\t", "\n", "\r\n", "\u{0B}", "\u{0C}", "\u{85}", "\u{A0}", "\u{1680}", "\u{2000}",
    "\u{200A}", "\u{2028}", "\u{2029}", "\u{202F}", "\u{205F}", "\u{3000}",
];
const NOT_SPACES: &[&str] = &["\u{200B}", "\u{FEFF}", "\u{180E}", "\u{1C}", "\u{0}"];

/// Strings with `base` in them, wrapped in (and broken by) spaces and near-spaces.
fn wrapped(bases: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for base in bases {
        for s in SPACES.iter().chain(NOT_SPACES) {
            out.push(format!("{s}{base}"));
            out.push(format!("{base}{s}"));
            out.push(format!("{s}{base}{s}"));
            let mid = base.len() / 2;
            if base.is_char_boundary(mid) && mid > 0 {
                out.push(format!("{}{s}{}", &base[..mid], &base[mid..]));
            }
        }
    }
    out
}

fn length_samples() -> Vec<String> {
    let zeros = |n: usize| "0".repeat(n);
    let numbers: Vec<String> = [
        "0", "00", "0.0", "0.", ".0", "000.000", "1", "01", "1.5", ".5", "5.", "12.25", "1e2",
        "1E2", "1e+2", "1e-2", "1.5e1", ".5e1", "5.e1", "0e9", "0e-999", "1e-50", "1e-99",
        "1e-100", "1e-400", "1e308", "1e309", "1e", "e1", ".", "..5", "1.2.3", "1,5", "\u{FF11}",
        "\u{663}", "Infinity", "inf", "NaN", "0x10", "1_0", "",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([
        format!("0.{}1", zeros(120)),
        format!("0.{}1", zeros(99)),
        format!("0.{}1e-80", zeros(250)),
        format!("0.{}1e-80", zeros(50)),
        format!("0.{}1e5", zeros(400)),
        format!("1{}", zeros(400)),
        format!("1{}e-700", zeros(400)),
    ])
    .collect();
    let units = [
        "", "px", "PX", "Px", "pt", "rem", "REM", "em", "in", "cm", "mm", "%", "fr", "FR", "vw",
        "ex", "p x", " px", "px ", "px%", "pxx", "pxpx",
    ];
    let mut out = Vec::new();
    for sign in ["", "+", "-", "\u{2212}"] {
        for n in &numbers {
            for u in units {
                out.push(format!("{sign}{n}{u}"));
            }
        }
    }
    let keywords = [
        "auto",
        "AUTO",
        "Auto",
        "au to",
        "aut",
        "autopx",
        "-auto",
        "xx-small",
        "x-small",
        "small",
        "medium",
        "large",
        "x-large",
        "xx-large",
        "xxx-large",
        "smaller",
        "larger",
        "LARGER",
        "Medium",
        "xxsmall",
        "medium%",
        "-small",
        "larger-",
        "inherit",
        "initial",
    ];
    out.extend(keywords.iter().map(|k| k.to_string()));
    out.extend(wrapped(&[
        "1px", "0", "auto", "50%", "1fr", "medium", "-1px", "2.5em",
    ]));
    out
}

fn color_samples() -> Vec<String> {
    let mut out: Vec<String> = [
        "red",
        "RED",
        "Red",
        "rebeccapurple",
        "transparent",
        "Transparent",
        "currentcolor",
        "notacolor",
        "red2",
        "re d",
        "#fff",
        "#FFF",
        "#ffff",
        "#ffffff",
        "#FfFfFf",
        "#ffffff00",
        "#ff",
        "#fffff",
        "#fffffff",
        "#fffffffff",
        "#ggg",
        "fff",
        "#",
        "##fff",
        "#fff#fff",
        "rgb(1,2,3)",
        "RGB(1, 2, 3)",
        "Rgb(1 2 3)",
        "rgba(1,2,3,0.5)",
        "rgb (1,2,3)",
        "rgb\u{3000}(1,2,3)",
        "rgb(1,2,3",
        "rgb1,2,3)",
        "rgb(1,2,3))",
        "rgb()",
        "rgb(300,0,0)",
        "rgb(a,b,c)",
        "hsl(0,0%,0%)",
        "hsla(0, 0%, 0%, 1)",
        "HSL(1,2%,3%)",
        "hsv(1,2,3)",
        "foo(1,2,3)",
        "rgbx(1,2,3)",
        "(1,2,3)",
        "",
        "-",
        "1",
        "red blue",
        "red,blue",
        "url(x)",
        "var(--c)",
        "rgb(1,2,3) red",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    out.extend(wrapped(&["red", "#fff", "rgb(1,2,3)", "transparent"]));
    out
}

/// What the color pattern takes that gridwell may not: a name (gridwell knows 148), or
/// a color function (gridwell checks its numbers).
fn color_form_only(s: &str) -> bool {
    let t = s.trim().to_ascii_lowercase();
    let function = ["rgb", "rgba", "hsl", "hsla"].iter().any(|f| {
        t.strip_prefix(f)
            .is_some_and(|r| r.trim_start().starts_with('(') && r.ends_with(')'))
    });
    (!t.is_empty() && t.chars().all(|c| c.is_ascii_alphabetic())) || function
}

/// What the length patterns take that gridwell doesn't: a number too big for a double,
/// and a negative one too small to tell from zero (to the schema, it may read as 0).
fn length_out_of_range(s: &str) -> bool {
    let t = s.trim().to_ascii_lowercase();
    let t = ["px", "pt", "rem", "em", "in", "cm", "mm", "%", "fr"]
        .iter()
        .find_map(|u| t.strip_suffix(u))
        .unwrap_or(&t);
    t.parse::<f64>()
        .is_ok_and(|v| v.is_infinite() || (v != 0.0 && v.abs() < 1e-99))
}

fn lang_samples() -> Vec<String> {
    // Tags well formed by RFC 5646 and not, at the edges of each subtag's length and
    // order.
    let tags = "
        en EN En en-US en-us ar qaa i-klingon zh-Hant-TW sr-Latn de-CH-1996 es-419 abcdefgh
        x-foo X-FOO x-a X-12345678 en-US-x-private en-a-bb en-a-bb-b-cc en-Latn-US-1996-a-bb-x-y
        abc-def-ghi-jkl en-abcdefgh en-0abc en-US-abcde-abcdef en-abc-Latn
        x x- x-123456789 e 1en a1 ab1 é en-é en.US en_US en- -en en--US en-1 en-12 es-41
        en-1234 en-abcd en-abcdefghi abcdefghi abc-def-ghi-jkl-mno abcd-def en-US-US
        en-Latn-Latn en-US-Latn abcdefgh-Latn en-a en-a-b en-a-bb-a-cc en-1-bb en-x en-x-a
        en-x-a-b
    ";
    let mut out: Vec<String> = tags.split_whitespace().map(str::to_string).collect();
    out.extend(["", "en US"].map(str::to_string));
    out.extend(wrapped(&["en", "fr-CA"]));
    out
}

fn table_id_samples() -> Vec<String> {
    let mut out: Vec<String> = ["a", "table-1", "T_1", "1", "é", "a.b", "a#b", "", "-"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    out.extend(wrapped(&["ab"]));
    out
}

fn href_samples() -> Vec<String> {
    let mut out: Vec<String> = [
        "https://example.com",
        "http://x",
        "HTTPS://X.ORG",
        "Http://x",
        "https://",
        "http://",
        "https:///x",
        "https://?q",
        "https://#f",
        "https://x/?q#f",
        "https:x",
        "https:/x",
        "httpss://x",
        "mailto:a@b",
        "mailto:",
        "MAILTO:a@b",
        "mailto:a",
        "javascript:alert(1)",
        "file:///etc/passwd",
        "ftp://x",
        "//x.org",
        "x",
        "",
        "data:text/html,x",
        "https://x\u{7F}",
        "https://x\u{9F}",
        "https://x\u{1}",
        "https://x/é",
        "https://[::1]/",
        "mailto:%20",
        "https://x/a%20b",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    out.extend(wrapped(&["https://x.org/", "mailto:a@b"]));
    out
}

/// Every value field's samples, by the value definition its schema refers to, and
/// what its pattern takes that gridwell doesn't.
#[allow(clippy::type_complexity)]
fn value_samples(def: &str) -> Option<(Vec<Value>, fn(&str) -> bool)> {
    let strings = |v: Vec<String>| v.into_iter().map(Value::String).collect::<Vec<_>>();
    let none: fn(&str) -> bool = |_| false;
    Some(match def {
        "column_width" | "size" | "extent" | "indent" | "border_width" | "font_size" => {
            (strings(length_samples()), length_out_of_range)
        }
        "color" => (strings(color_samples()), color_form_only),
        "lang" => (strings(lang_samples()), none),
        "table_id" => (strings(table_id_samples()), none),
        "href" => (strings(href_samples()), none),
        "font_weight" => {
            let mut v = strings(
                [
                    "normal", "bold", "bolder", "lighter", "BOLD", " bold ", "100", "700", "1000",
                    "1", "0", "1001", "0700", "+700", " 700", "700.0", "7e2", "", "x", "semibold",
                    "-1",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            );
            v.extend([700, 1, 1000, 0, 1001, -1].map(|n| json!(n)));
            v.extend([700.5, 700.0, 1e3].map(|n| json!(n)));
            (v, none)
        }
        _ => return None,
    })
}

#[test]
fn every_value_field_fits_as_gridwell_reads_it() {
    let fields: Vec<(Field, String)> = fields()
        .into_iter()
        .filter_map(|f| {
            let def = value_def(f.schema()?)?.to_string();
            (!matches!(def.as_str(), "style_ref" | "style_id")).then_some((f, def))
        })
        .collect();
    let kinds: BTreeSet<&str> = fields.iter().map(|(_, d)| d.as_str()).collect();
    assert_eq!(
        kinds,
        BTreeSet::from([
            "border_width",
            "color",
            "column_width",
            "extent",
            "font_size",
            "font_weight",
            "href",
            "indent",
            "lang",
            "size",
            "table_id"
        ])
    );
    let mut cases = Vec::new();
    for (f, def) in &fields {
        let (samples, loose) = value_samples(def).unwrap_or_else(|| panic!("no samples for {def}"));
        cases.extend(
            samples
                .into_iter()
                .map(|s| (f.clone(), def.clone(), s, loose)),
        );
    }
    let results = par(&cases, |(f, def, sample, loose)| {
        let v = f.with(Some(sample.clone()));
        let fit = fits(&v);
        let at = format!("{} ({def}) = {sample}", f.name());
        let mut out = Vec::new();
        match verdict(&v) {
            Verdict::Accepted(back) => {
                if !fits(&back) {
                    out.push(Err(format!(
                        "{at}: gridwell accepts it, but written back it doesn't fit"
                    )));
                }
                // A value gridwell writes back as it is must fit as it is.
                if f.of(&back) == *sample && !fit {
                    out.push(Err(format!(
                        "{at}: gridwell accepts it; the schema doesn't"
                    )));
                }
                out.push(Ok((f.name(), true)));
            }
            Verdict::Unparsed => {
                if fit {
                    out.push(Err(format!("{at}: fits the schema but doesn't parse")));
                }
                out.push(Ok((f.name(), false)));
            }
            Verdict::Invalid(rules) => {
                let mine = rules
                    .iter()
                    .all(|r| FIELD_RULES.contains(r) || *r == ValidationRule::InvalidColor);
                if fit && mine && !sample.as_str().is_some_and(loose) {
                    out.push(Err(format!(
                        "{at}: fits the schema, but gridwell rejects it: {rules:?}"
                    )));
                }
                out.push(Ok((f.name(), false)));
            }
        }
        out
    });
    let mut problems = Vec::new();
    let mut tally: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for r in results {
        match r {
            Err(p) => problems.push(p),
            Ok((name, accepted)) => {
                let t = tally.entry(name).or_default();
                if accepted {
                    t.0 += 1
                } else {
                    t.1 += 1
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
    // Each field took some samples and refused others.
    for (name, (accepted, rejected)) in &tally {
        assert!(
            *accepted > 3 && *rejected > 3,
            "{name}: {accepted} accepted, {rejected} rejected"
        );
    }
}

#[test]
fn style_ids_fit_as_gridwell_reads_them() {
    let ids = [
        "a",
        "A-z_0",
        "new-id",
        "_",
        "-",
        "",
        "a b",
        "a.b",
        "a:b",
        "é",
        "a\n",
        "a/b",
        "a~b",
        "a\u{3000}",
    ];
    let max = maximal();
    let base = max["styles"]["defs"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    for id in ids {
        for (map, entry) in [
            ("defs", json!({})),
            ("compositions", json!({ "extends": base })),
        ] {
            let mut v = max.clone();
            v["styles"][map][id] = entry;
            let accepted = matches!(verdict(&v), Verdict::Accepted(_));
            assert_eq!(
                fits(&v),
                accepted,
                "styles.{map}[{id:?}]: fits {}, accepted {accepted}",
                fits(&v)
            );
        }
    }
}

#[test]
fn typed_values_fit_as_gridwell_reads_them() {
    let values = [
        json!("PT1H30M"),
        json!("P1Y2M3W4DT5H6M7.5S"),
        json!("-P3D"),
        json!("P0D"),
        json!("PT0.25S"),
        json!("P"),
        json!("-P"),
        json!("PT"),
        json!("P1DT"),
        json!("PT1.5M"),
        json!("P1H"),
        json!("PT1D"),
        json!("P1M1Y"),
        json!("pt1h"),
        json!(" PT1H"),
        json!("PT1H "),
        json!("P-1D"),
        json!("+P1D"),
        json!("P1.D"),
        json!("PT.5S"),
        json!("P１D"),
        json!("1:30:00"),
        json!(""),
        json!("text"),
        json!(5400),
        json!(1.5),
        json!(true),
        json!([]),
        json!({}),
    ];
    let max = maximal();
    let mut tally = BTreeMap::new();
    for ty in [
        "string", "duration", "number", "integer", "boolean", "date", "other",
    ] {
        for value in &values {
            let mut v = max.clone();
            v["table"]["tbody"][0]["rows"][0]["cells"][1]["typed_value"] =
                json!({ "type": ty, "value": value });
            let accepted = matches!(verdict(&v), Verdict::Accepted(_));
            assert_eq!(
                fits(&v),
                accepted,
                "{ty} {value}: fits {}, accepted {accepted}",
                fits(&v)
            );
            *tally.entry((ty, accepted)).or_insert(0) += 1;
        }
    }
    // Not vacuous: each of the checked types takes some values and refuses others.
    for ty in ["string", "duration"] {
        assert!(
            tally[&(ty, true)] > 0 && tally[&(ty, false)] > 0,
            "{tally:?}"
        );
    }
    assert_eq!(tally[&("duration", true)], 5, "{tally:?}");
}

// --- Compatibility -----------------------------------------------------------------

/// The tables of `compat/`, by the version they were written for.
fn compat_tables() -> Vec<(IrVersion, String, Value)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root().join("compat")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let Some(version) = name.strip_prefix("ir-").and_then(IrVersion::parse) else {
            continue;
        };
        let mut dirs = vec![path];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                let file = path.to_string_lossy().into_owned();
                if path.is_dir() {
                    dirs.push(path);
                } else if file.ends_with(".json") && !file.ends_with(".grid.json") {
                    let v = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                    out.push((version, file, v));
                }
            }
        }
    }
    out
}

#[test]
fn every_frozen_table_fits_every_schema_of_its_major_version_since() {
    let tables = compat_tables();
    assert!(tables.len() > 90, "{} compat tables", tables.len());
    for (version, name) in schema_files() {
        let text = read(&format!("{SCHEMA_DIR}/{name}"));
        let validator = compile(&serde_json::from_str(&text).unwrap());
        for (written_for, file, table) in &tables {
            if written_for.major != version.major || *written_for > version {
                continue;
            }
            let back = written_back(table).unwrap_or_else(|| panic!("{file} doesn't parse"));
            assert!(
                validator.is_valid(&back),
                "{file}, written back, doesn't fit {name}"
            );
            assert!(validator.is_valid(table), "{file} doesn't fit {name}");
        }
    }
}

// --- Widening ----------------------------------------------------------------------

/// Keywords that don't constrain.
const ANNOTATIONS: &[&str] = &[
    "$schema",
    "$id",
    "title",
    "description",
    "$comment",
    "examples",
];
/// Keywords the check compares.
const COMPARED: &[&str] = &[
    "$ref",
    "$defs",
    "type",
    "enum",
    "const",
    "required",
    "minimum",
    "maximum",
    "pattern",
    "properties",
    "additionalProperties",
    "propertyNames",
    "items",
    "anyOf",
    "not",
    "default",
];
/// Keywords that constrain when they are added.
const CONSTRAINING: &[&str] = &[
    "$ref",
    "type",
    "enum",
    "const",
    "required",
    "minimum",
    "maximum",
    "pattern",
    "additionalProperties",
    "propertyNames",
    "items",
    "anyOf",
    "not",
];

/// How `new` takes less than `old`, or means something else by the same table: each
/// a place in the schema and what changed. Empty when `new` only widens `old`, as a
/// minor version may: it adds fields, definitions, keyword values, node types (the
/// last `anyOf` branches) and looser patterns.
fn narrowings(old: &Value, new: &Value) -> Vec<String> {
    let mut out = Vec::new();
    narrow(old, new, "", &mut out);
    out
}

fn narrow(old: &Value, new: &Value, path: &str, out: &mut Vec<String>) {
    let (Some(o), Some(n)) = (old.as_object(), new.as_object()) else {
        if old != new {
            out.push(format!("{path}: changed"));
        }
        return;
    };
    for key in n.keys() {
        if !ANNOTATIONS.contains(&key.as_str()) && !COMPARED.contains(&key.as_str()) {
            out.push(format!(
                "{path}: `{key}` is a keyword this check doesn't compare"
            ));
        } else if CONSTRAINING.contains(&key.as_str())
            && !o.contains_key(key)
            && n[key] != json!(true)
        {
            out.push(format!("{path}: adds `{key}`"));
        }
    }
    for (key, ov) in o {
        let p = format!("{path}/{key}");
        let Some(nv) = n.get(key) else {
            // Dropping a constraint widens; dropping a field, a definition or a
            // default changes what a table means.
            if matches!(key.as_str(), "properties" | "$defs" | "default") {
                out.push(format!("{p}: removed"));
            }
            continue;
        };
        match key.as_str() {
            "$ref" | "const" => {
                if ov != nv {
                    out.push(format!("{p}: changed"));
                }
            }
            "default" => {
                if ov != nv {
                    out.push(format!(
                        "{p}: changed, so a table without the field means something else"
                    ));
                }
            }
            "type" => {
                let types = |v: &Value| -> BTreeSet<String> {
                    match v {
                        Value::Array(a) => {
                            a.iter().map(|t| t.as_str().unwrap().to_string()).collect()
                        }
                        t => BTreeSet::from([t.as_str().unwrap().to_string()]),
                    }
                };
                for t in types(ov).difference(&types(nv)) {
                    out.push(format!("{p}: no longer takes {t}"));
                }
            }
            "enum" => {
                for v in ov.as_array().unwrap() {
                    if !nv.as_array().unwrap().contains(v) {
                        out.push(format!("{p}: no longer takes {v}"));
                    }
                }
            }
            "required" => {
                for v in nv.as_array().unwrap() {
                    if !ov.as_array().unwrap().contains(v) {
                        out.push(format!("{p}: now requires {v}"));
                    }
                }
            }
            "minimum" if nv.as_f64() > ov.as_f64() => out.push(format!("{p}: raised")),
            "maximum" if nv.as_f64() < ov.as_f64() => out.push(format!("{p}: lowered")),
            "pattern" if ov != nv => {
                let (old, new) = (pattern(ov), pattern(nv));
                if let Some(s) = pattern_samples()
                    .iter()
                    .find(|s| old.is_valid(s) && !new.is_valid(s))
                {
                    out.push(format!("{p}: no longer matches {s}"));
                }
            }
            "properties" | "$defs" => {
                for (k, osub) in ov.as_object().unwrap() {
                    match nv.get(k) {
                        None => out.push(format!("{p}/{k}: removed")),
                        Some(nsub) => narrow(osub, nsub, &format!("{p}/{k}"), out),
                    }
                }
            }
            "additionalProperties" | "propertyNames" | "items" => narrow(ov, nv, &p, out),
            "anyOf" => {
                // A branch that names a definition is found by its name, wherever
                // it is; one that doesn't, by its place.
                let (oa, na) = (ov.as_array().unwrap(), nv.as_array().unwrap());
                for (i, ob) in oa.iter().enumerate() {
                    let nb = match ref_name(ob) {
                        Some(name) => na.iter().find(|nb| ref_name(nb) == Some(name)),
                        None => na.get(i).filter(|nb| ref_name(nb).is_none()),
                    };
                    match nb {
                        Some(nb) => narrow(ob, nb, &format!("{p}/{i}"), out),
                        None => out.push(format!("{p}/{i}: removed")),
                    }
                }
            }
            "not" => {
                // An unknown node's type is any but the known ones, which a new node type
                // joins: a node type, like a field, is checked once it is added.
                let mut grown = nv.clone();
                if let (Some(oe), Some(ne)) = (ov["enum"].as_array(), grown.get_mut("enum")) {
                    if oe.iter().all(|t| ne.as_array().unwrap().contains(t)) {
                        *ne = json!(oe);
                    }
                }
                if *ov != grown {
                    out.push(format!("{p}: changed"));
                }
            }
            _ => {}
        }
    }
}

fn pattern(p: &Value) -> Validator {
    compile(&json!({ "pattern": p }))
}

/// Strings to try a changed pattern on: every value sample, and every string of the
/// frozen tables.
fn pattern_samples() -> &'static [Value] {
    static SAMPLES: OnceLock<Vec<Value>> = OnceLock::new();
    SAMPLES.get_or_init(|| {
        fn strings(v: &Value, out: &mut BTreeSet<String>) {
            match v {
                Value::String(s) => {
                    out.insert(s.clone());
                }
                Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
                Value::Object(m) => m.iter().for_each(|(k, x)| {
                    out.insert(k.clone());
                    strings(x, out)
                }),
                _ => {}
            }
        }
        let mut all = BTreeSet::new();
        for list in [
            length_samples(),
            color_samples(),
            lang_samples(),
            table_id_samples(),
            href_samples(),
        ] {
            all.extend(list);
        }
        for (_, _, table) in compat_tables() {
            strings(&table, &mut all);
        }
        all.into_iter().map(Value::String).collect()
    })
}

#[test]
fn no_schema_takes_less_than_an_earlier_one() {
    let files = schema_files();
    let schemas: Vec<(IrVersion, String, Value)> = files
        .into_iter()
        .map(|(v, n)| {
            let s = serde_json::from_str(&read(&format!("{SCHEMA_DIR}/{n}"))).unwrap();
            (v, n, s)
        })
        .collect();
    for (i, (v_old, n_old, old)) in schemas.iter().enumerate() {
        for (v_new, n_new, new) in &schemas[i..] {
            if v_old.major == v_new.major {
                let found = narrowings(old, new);
                assert!(
                    found.is_empty(),
                    "{n_new} takes less than {n_old}:\n{}",
                    found.join("\n")
                );
            }
        }
    }
}

fn defs(s: &mut Value) -> &mut Value {
    &mut s["$defs"]
}

/// Edits a copy of the current schema.
fn edited(edit: impl FnOnce(&mut Value)) -> Value {
    let mut s = schema().clone();
    edit(&mut s);
    s
}

#[test]
fn the_widening_check_finds_every_kind_of_narrowing() {
    let narrowed: Vec<(&str, Value)> = vec![
        (
            "a keyword value removed",
            edited(|s| {
                defs(s)["column"]["properties"]["align"]["enum"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|v| v != "justify")
            }),
        ),
        (
            "a field required",
            edited(|s| {
                defs(s)["column"]["required"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("label"))
            }),
        ),
        (
            "a type removed",
            edited(|s| defs(s)["style_def"]["properties"]["font_family"]["type"] = json!("string")),
        ),
        (
            "a minimum raised",
            edited(|s| defs(s)["cell"]["properties"]["colspan"]["minimum"] = json!(2)),
        ),
        (
            "a maximum lowered",
            edited(|s| defs(s)["config"]["properties"]["table_cols"]["maximum"] = json!(1000)),
        ),
        (
            "a pattern narrowed",
            edited(|s| {
                let p = defs(s)["size"]["pattern"]
                    .as_str()
                    .unwrap()
                    .replace("[Aa][Uu][Tt][Oo]", "nope");
                defs(s)["size"]["pattern"] = json!(p);
            }),
        ),
        (
            "fields closed",
            edited(|s| defs(s)["cell"]["additionalProperties"] = json!(false)),
        ),
        (
            "a field removed",
            edited(|s| {
                defs(s)["column"]["properties"]
                    .as_object_mut()
                    .unwrap()
                    .remove("hidden");
            }),
        ),
        (
            "a definition removed",
            edited(|s| {
                defs(s).as_object_mut().unwrap().remove("rules");
            }),
        ),
        (
            "a default changed",
            edited(|s| defs(s)["column"]["properties"]["width"]["default"] = json!("1fr")),
        ),
        (
            "an unknown keyword",
            edited(|s| defs(s)["text_node"]["properties"]["value"]["maxLength"] = json!(10)),
        ),
        (
            "a node type removed",
            edited(|s| {
                defs(s)["content_node"]["anyOf"]
                    .as_array_mut()
                    .unwrap()
                    .remove(4);
            }),
        ),
        (
            "a reference changed",
            edited(|s| {
                defs(s)["config"]["properties"]["lang"]["anyOf"][0]["$ref"] =
                    json!("#/$defs/table_id")
            }),
        ),
        (
            "a constraint added",
            edited(|s| defs(s)["config"]["properties"]["locale"]["enum"] = json!(["en-US"])),
        ),
        (
            "a const changed",
            edited(|s| defs(s)["raw_node"]["properties"]["type"]["const"] = json!("markup")),
        ),
        (
            "a maximum added",
            edited(|s| defs(s)["typed_value"]["properties"]["value"]["maximum"] = json!(1)),
        ),
        (
            "a version pattern narrowed",
            edited(|s| s["properties"]["ir_version"]["pattern"] = json!("^1\\.0$")),
        ),
    ];
    for (what, new) in &narrowed {
        assert!(!narrowings(schema(), new).is_empty(), "not found: {what}");
    }
    let widened: Vec<(&str, Value)> = vec![
        (
            "a keyword value added",
            edited(|s| {
                defs(s)["column"]["properties"]["align"]["enum"]
                    .as_array_mut()
                    .unwrap()
                    .insert(0, json!("start"))
            }),
        ),
        (
            "a field added",
            edited(|s| {
                defs(s)["column"]["properties"]["note"] =
                    json!({"type": ["string", "null"], "default": null})
            }),
        ),
        (
            "a definition added",
            edited(|s| defs(s)["later"] = json!({"type": "object"})),
        ),
        (
            "a type added",
            edited(|s| {
                defs(s)["cell"]["properties"]["colspan"]["type"] = json!(["integer", "null"])
            }),
        ),
        (
            "a maximum raised",
            edited(|s| defs(s)["config"]["properties"]["table_cols"]["maximum"] = json!(1e10)),
        ),
        (
            "a requirement dropped",
            edited(|s| defs(s)["footnote"]["required"] = json!(["id", "content"])),
        ),
        (
            "a description changed",
            edited(|s| defs(s)["cell"]["description"] = json!("A cell of the table.")),
        ),
        (
            "a node type added",
            edited(|s| {
                defs(s)["chart_node"] = json!({"type": "object", "required": ["type"], "properties": {"type": {"const": "chart"}}});
                defs(s)["content_node"]["anyOf"]
                    .as_array_mut()
                    .unwrap()
                    .insert(6, json!({"$ref": "#/$defs/chart_node"}));
                defs(s)["unknown_node"]["properties"]["type"]["not"]["enum"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("chart"));
            }),
        ),
        (
            "a pattern widened",
            edited(|s| {
                let p = defs(s)["lang"]["pattern"]
                    .as_str()
                    .unwrap()
                    .replace("{2,8}", "{2,9}");
                defs(s)["lang"]["pattern"] = json!(p);
            }),
        ),
    ];
    for (what, new) in &widened {
        let found = narrowings(schema(), new);
        assert!(
            found.is_empty(),
            "{what} is taken for a narrowing: {found:?}"
        );
    }
}
