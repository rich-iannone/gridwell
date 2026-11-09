//! The documentation says what the code does. Each IR object has a field table
//! (marked `<!-- ir-fields: Type -->`) listing exactly its fields, with the right
//! defaults; the keyword, color and length tables list exactly the fields that
//! are checked and the values they take; the rule tables name every validation
//! rule once; the error lines shown are what the validator writes; every JSON
//! example fits the IR; every link between pages lands on a page and a heading
//! that exist; and the formats the docs call experimental are the registry's.
//!
//! The IR's shape comes from `docs/maximal.json`, a valid table with every field
//! set: parsed and written back, it has every field of every object.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gridwell_ir::{
    BorderStyle, CellScope, FontStyle, FontWeight, HAlign, Keyword, Overflow, PageBreakMode,
    RowParity, RowRole, SelectorScope, Table, TextDecoration, TextDirection, TextOverflow,
    TextTransform, VAlign, ValidationRule, ValueType, WhiteSpace, WordBreak,
};
use serde_json::{json, Value};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// Every documentation page, by path from the repository root.
fn pages() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut dirs = vec![root().join("docs")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if !path.ends_with("_site") && !path.ends_with(".quarto") {
                    dirs.push(path);
                }
            } else if path.extension().is_some_and(|e| e == "qmd") {
                let rel = path.strip_prefix(root()).unwrap();
                let rel = rel.to_string_lossy().replace('\\', "/");
                out.push((rel.clone(), read(&rel)));
            }
        }
    }
    for rel in [
        "README.md",
        "crates/gridwell/README.md",
        "crates/gridwell-cli/README.md",
        "conformance/README.md",
    ] {
        out.push((rel.to_string(), read(rel)));
    }
    out.sort();
    out
}

fn page(rel: &str) -> String {
    read(rel)
}

// --- Markdown ------------------------------------------------------------------

/// A fenced code block.
struct Block {
    line: usize,
    info: String,
    body: String,
}

/// The fenced code blocks of a page (``` fences; longer fences hold shorter ones).
fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut lines = text.lines().enumerate();
    while let Some((i, line)) = lines.next() {
        let trimmed = line.trim_start();
        let ticks = trimmed.chars().take_while(|&c| c == '`').count();
        if ticks < 3 {
            continue;
        }
        let fence = &trimmed[..ticks];
        let info = trimmed[ticks..].trim().to_string();
        let mut body = String::new();
        for (_, l) in lines.by_ref() {
            if l.trim() == fence {
                break;
            }
            body.push_str(l);
            body.push('\n');
        }
        out.push(Block {
            line: i + 1,
            info,
            body,
        });
    }
    out
}

/// A block's language: `json` for ```` ```json ```` and ```` ```{.json …} ````.
fn lang(info: &str) -> &str {
    let info = info.trim_start_matches('{').trim_start_matches('.');
    info.split(|c: char| c.is_whitespace() || c == '}')
        .next()
        .unwrap_or("")
}

/// The text outside code blocks, line by line, with the line numbers.
fn prose(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut fence: Option<String> = None;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let ticks = trimmed.chars().take_while(|&c| c == '`').count();
        match &fence {
            Some(f) if trimmed.trim_end() == f => fence = None,
            Some(_) => {}
            None if ticks >= 3 => fence = Some(trimmed[..ticks].to_string()),
            None => out.push((i + 1, line)),
        }
    }
    out
}

/// A Markdown table, with the HTML comment just above it if there is one.
struct MdTable {
    line: usize,
    marker: Option<String>,
    header: Vec<String>,
    rows: Vec<Vec<String>>,
}

/// Splits a table row on `|`, leaving those in code spans and escaped ones alone.
fn cells(line: &str) -> Vec<String> {
    let line = line.trim();
    let line = line.strip_prefix('|').unwrap_or(line);
    let line = line.strip_suffix('|').unwrap_or(line);
    let mut out = Vec::new();
    let mut cell = String::new();
    let mut code = false;
    let mut prev = ' ';
    for c in line.chars() {
        match c {
            '`' => {
                code = !code;
                cell.push(c);
            }
            '|' if !code && prev != '\\' => out.push(std::mem::take(&mut cell)),
            _ => cell.push(c),
        }
        prev = c;
    }
    out.push(cell);
    out.into_iter().map(|c| c.trim().to_string()).collect()
}

fn tables(text: &str) -> Vec<MdTable> {
    let lines = prose(text);
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let (n, line) = lines[i];
        if !line.trim_start().starts_with('|') {
            i += 1;
            continue;
        }
        // The comment above it, past blank lines.
        let marker = lines[..i]
            .iter()
            .rev()
            .find(|(_, l)| !l.trim().is_empty())
            .and_then(|(_, l)| {
                let l = l.trim();
                l.strip_prefix("<!--")
                    .and_then(|l| l.strip_suffix("-->"))
                    .map(|l| l.trim().to_string())
            });
        let header = cells(line);
        let mut rows = Vec::new();
        i += 2; // the header and the separator
        while i < lines.len() && lines[i].1.trim_start().starts_with('|') {
            rows.push(cells(lines[i].1));
            i += 1;
        }
        out.push(MdTable {
            line: n,
            marker,
            header,
            rows,
        });
    }
    out
}

/// The `code spans` of some Markdown, with whether each is inside parentheses.
fn code_spans(s: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut rest = s;
    while let Some(start) = rest.find(['`', '(', ')']) {
        let c = rest[start..].chars().next().unwrap();
        match c {
            '(' => {
                depth += 1;
                rest = &rest[start + 1..];
            }
            ')' => {
                depth -= 1;
                rest = &rest[start + 1..];
            }
            _ => {
                let after = &rest[start + 1..];
                let end = after.find('`').expect("unclosed code span");
                out.push((after[..end].to_string(), depth > 0));
                rest = &after[end + 1..];
            }
        }
    }
    out
}

/// The first code span of a cell (`` `name` `` → `name`).
fn code(cell: &str) -> Option<String> {
    code_spans(cell).into_iter().next().map(|(s, _)| s)
}

// --- The IR's shape ---------------------------------------------------------------

/// Which object each path holds. A path's segments are object keys, with `[]` for
/// an array's items and `*` for a map's entries; `**` matches any run of segments.
const TYPES: &[(&str, &str)] = &[
    ("", "Table"),
    ("config", "Config"),
    ("styles", "StylePalette"),
    ("styles.defs.*", "StyleDef"),
    ("styles.compositions.*", "StyleComposition"),
    ("styles.compositions.*.overrides", "StyleDef"),
    ("styles.conditionals[]", "ConditionalStyle"),
    ("styles.conditionals[].selector", "ConditionalSelector"),
    ("styles.conditionals[].style", "StyleDef"),
    ("**.padding", "Padding"),
    ("**.border", "BorderSet"),
    ("**.border.*", "Border"),
    ("header", "Header"),
    ("header.title", "HeaderLine"),
    ("header.subtitle", "HeaderLine"),
    ("header.extra_lines[]", "HeaderLine"),
    ("column_spec[]", "ColumnSpec"),
    ("table", "TableBlock"),
    ("table.thead", "TableHead"),
    ("table.tbody[]", "RowGroup"),
    ("table.tbody[].label", "GroupLabel"),
    ("**.rows[]", "Row"),
    ("**.summary_rows[]", "Row"),
    ("**.cells[]", "Cell"),
    ("**.typed_value", "TypedValue"),
    ("footer", "Footer"),
    ("footer.footnotes[]", "Footnote"),
    ("footer.source_notes[]", "SourceNote"),
    // Content nodes are told apart by their `type`: `content:text`, …
    ("**.content[]", "content"),
];

/// Values that are any JSON, not IR objects.
const FREE: &[&str] = &[
    "extensions",
    "header.preheader_content",
    "**.sort_key",
    "**.typed_value.value",
];

/// Objects whose keys are names (a map), not fields.
const MAPS: &[&str] = &["styles.defs", "styles.compositions"];

/// The one null in `maximal.json`: a data row has no role.
const NULLS: &[&str] = &["table.tbody[].rows[].role"];

fn matches(pattern: &str, path: &str) -> bool {
    fn go(p: &[&str], s: &[&str]) -> bool {
        match p.split_first() {
            None => s.is_empty(),
            Some((&"**", rest)) => (0..=s.len()).any(|i| go(rest, &s[i..])),
            Some((head, rest)) => s
                .split_first()
                .is_some_and(|(h, t)| (*head == "*" || head == h) && go(rest, t)),
        }
    }
    fn split(x: &str) -> Vec<&str> {
        if x.is_empty() {
            vec![]
        } else {
            x.split('.').collect()
        }
    }
    go(&split(pattern), &split(path))
}

fn type_of(path: &str) -> Option<&'static str> {
    TYPES
        .iter()
        .find(|(p, _)| matches(p, path))
        .map(|(_, t)| *t)
}

fn join(path: &str, seg: &str) -> String {
    if path.is_empty() {
        seg.to_string()
    } else {
        format!("{path}.{seg}")
    }
}

/// One object of the IR as it appears in a table: its fields, and where.
#[derive(Default)]
struct Shape {
    /// Type → its fields.
    fields: BTreeMap<String, BTreeSet<String>>,
    /// Type → the JSON pointer of its first object, and its path.
    first: BTreeMap<String, (String, String)>,
    /// Every object's path, pointer and type.
    objects: Vec<(String, String, String)>,
}

impl Shape {
    fn of(v: &Value) -> Shape {
        let mut shape = Shape::default();
        shape.walk(v, "", "");
        shape
    }

    fn walk(&mut self, v: &Value, path: &str, pointer: &str) {
        if FREE.iter().any(|p| matches(p, path)) {
            return;
        }
        match v {
            Value::Object(map) if MAPS.contains(&path) => {
                for (k, item) in map {
                    self.walk(
                        item,
                        &join(path, "*"),
                        &format!("{pointer}/{}", k.replace('~', "~0").replace('/', "~1")),
                    );
                }
            }
            Value::Object(map) => {
                let ty = type_of(path).unwrap_or_else(|| {
                    panic!("no type for the object at `{path}`: add it to TYPES and document it")
                });
                let ty = if ty == "content" {
                    format!("content:{}", map["type"].as_str().unwrap())
                } else {
                    ty.to_string()
                };
                let keys: BTreeSet<String> = map.keys().cloned().collect();
                if let Some(known) = self.fields.get(&ty) {
                    assert_eq!(known, &keys, "{ty} has other fields at `{path}`");
                } else {
                    self.fields.insert(ty.clone(), keys);
                    self.first
                        .insert(ty.clone(), (pointer.to_string(), path.to_string()));
                }
                self.objects
                    .push((path.to_string(), pointer.to_string(), ty.clone()));
                for (k, item) in map {
                    self.walk(item, &join(path, k), &format!("{pointer}/{k}"));
                }
            }
            Value::Array(items) => {
                assert!(!items.is_empty(), "`{path}` is empty in maximal.json");
                let item_path = format!("{path}[]");
                for (i, item) in items.iter().enumerate() {
                    self.walk(item, &item_path, &format!("{pointer}/{i}"));
                }
            }
            Value::Null => assert!(
                NULLS.contains(&path),
                "`{path}` is null in maximal.json: set it, so its fields are seen"
            ),
            _ => {}
        }
    }
}

/// `maximal.json`, as the IR writes it back (every field, in order).
fn maximal() -> Value {
    let text = read("xtask/tests/docs/maximal.json");
    let table = Table::from_json(&text).unwrap();
    let errors = table.validate();
    assert!(errors.is_empty(), "maximal.json: {errors:?}");
    serde_json::to_value(&table).unwrap()
}

/// Checks a match over every content node type, so a new one isn't missed here.
#[allow(dead_code)]
fn content_node_types(n: &gridwell_ir::ContentNode) -> &'static str {
    use gridwell_ir::ContentNode as N;
    match n {
        N::Text { .. } => "text",
        N::StyledText { .. } => "styled_text",
        N::LineBreak {} => "line_break",
        N::FootnoteMark { .. } => "footnote_mark",
        N::Image { .. } => "image",
        N::Raw { .. } => "raw",
        N::Unknown(_) => "unknown",
    }
}

const CONTENT_TYPES: &[&str] = &[
    "text",
    "styled_text",
    "line_break",
    "footnote_mark",
    "image",
    "raw",
];

#[test]
fn maximal_json_has_every_field() {
    let shape = Shape::of(&maximal());
    let seen: BTreeSet<&str> = shape
        .fields
        .keys()
        .filter_map(|t| t.strip_prefix("content:"))
        .collect();
    assert_eq!(seen, CONTENT_TYPES.iter().copied().collect());
    // Every type in TYPES has an object in the table.
    for (_, ty) in TYPES {
        if *ty != "content" {
            assert!(shape.fields.contains_key(*ty), "no {ty} in maximal.json");
        }
    }
    // Every keyword field holds a known value, so a typo isn't read as a test of
    // UNKNOWN_VALUE.
    assert!(Table::from_json(&maximal().to_string())
        .unwrap()
        .validate()
        .is_empty());
}

// --- Field tables -----------------------------------------------------------------

/// Parses a table with one field of `maximal` removed: `None` if that's an error
/// (the field is required), else the value it takes.
fn default_of(max: &Value, pointer: &str, key: &str) -> Option<Value> {
    let mut v = max.clone();
    v.pointer_mut(pointer)
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove(key)
        .unwrap();
    let table = Table::from_json(&v.to_string()).ok()?;
    let back = serde_json::to_value(&table).unwrap();
    Some(back.pointer(&format!("{pointer}/{key}")).unwrap().clone())
}

/// A value without its null members: the IR writes every unset field as null,
/// which a documented default leaves out.
fn without_nulls(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k.clone(), without_nulls(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(without_nulls).collect()),
        v => v.clone(),
    }
}

/// The JSON kind of a value, as the field tables write it.
fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_u64() || n.is_i64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// The JSON kinds a field table's Type allows: `string`, `length`, `color` and
/// `keyword` are strings, `array of …` an array, `map …`, `object` or an IR
/// object's name an object; `A or B` either; `?` at the end, null too.
fn doc_kinds(cell: &str, shape: &Shape) -> Result<Vec<&'static str>, String> {
    cell.trim_end_matches('?')
        .split(" or ")
        .map(|part| {
            let word = part
                .trim()
                .trim_start_matches(['[', '`'])
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap_or("");
            Ok(match word {
                "string" | "length" | "color" | "keyword" => "string",
                "integer" => "integer",
                "number" => "number",
                "boolean" => "boolean",
                "array" => "array",
                "object" | "map" => "object",
                "any" => "any",
                w if shape.fields.contains_key(w) => "object",
                w => return Err(w.to_string()),
            })
        })
        .collect()
}

#[test]
fn every_ir_object_has_a_field_table_listing_its_fields() {
    let max = maximal();
    let shape = Shape::of(&max);
    let mut documented: BTreeMap<String, String> = BTreeMap::new();
    let mut problems = Vec::new();
    for (rel, text) in pages() {
        for t in tables(&text) {
            let Some(ty) = t
                .marker
                .as_deref()
                .and_then(|m| m.strip_prefix("ir-fields:"))
            else {
                continue;
            };
            let ty = ty.trim().to_string();
            let at = format!("{rel}:{}", t.line);
            if let Some(other) = documented.insert(ty.clone(), at.clone()) {
                problems.push(format!("{at}: {ty} is documented at {other} too"));
            }
            let Some(fields) = shape.fields.get(&ty) else {
                problems.push(format!("{at}: no IR object {ty}"));
                continue;
            };
            if t.header != ["Field", "Type", "Default", "Description"] {
                problems.push(format!(
                    "{at}: the columns are {:?}, not Field | Type | Default | Description",
                    t.header
                ));
                continue;
            }
            let (pointer, _) = &shape.first[&ty];
            let mut listed = BTreeSet::new();
            for row in &t.rows {
                let Some(name) = code(&row[0]) else {
                    problems.push(format!("{at}: a row without a `field`: {row:?}"));
                    continue;
                };
                if !listed.insert(name.clone()) {
                    problems.push(format!("{at}: {name} is listed twice"));
                }
                if !fields.contains(&name) {
                    problems.push(format!("{at}: {ty} has no field {name}"));
                    continue;
                }
                let (ty_cell, default_cell) = (&row[1], &row[2]);
                let default = default_of(&max, pointer, &name);
                match (&default, default_cell.as_str()) {
                    (None, "required") => {}
                    (None, other) => {
                        problems.push(format!("{at}: {name} is required, not {other}"))
                    }
                    (Some(d), cell) => {
                        match code(cell).map(|c| serde_json::from_str::<Value>(&c)) {
                            Some(Ok(doc)) if without_nulls(&doc) == without_nulls(d) => {}
                            _ => problems.push(format!(
                                "{at}: {name} defaults to `{d}`, documented as {cell}"
                            )),
                        }
                    }
                }
                // The type: the JSON kind of the value, `?` when it may be null.
                let nullable = default == Some(Value::Null);
                if nullable != ty_cell.ends_with('?') {
                    problems.push(format!(
                        "{at}: {name}'s type {ty_cell} {} end in `?`",
                        if nullable { "should" } else { "shouldn't" }
                    ));
                }
                // Every value it has in maximal.json is of a kind the type says.
                let kinds = match doc_kinds(ty_cell, &shape) {
                    Ok(kinds) => kinds,
                    Err(word) => {
                        problems.push(format!("{at}: {name}'s type {ty_cell}: what's {word}?"));
                        continue;
                    }
                };
                for p in places(&shape, &ty, &name) {
                    let actual = kind(max.pointer(&p).unwrap());
                    let fits = actual == "null"
                        || kinds.iter().any(|k| {
                            *k == "any" || *k == actual || (*k == "number" && actual == "integer")
                        });
                    if !fits {
                        problems.push(format!(
                            "{at}: {name} is {actual} at {p} in maximal.json, documented as {ty_cell}"
                        ));
                    }
                }
            }
            for missing in fields.difference(&listed) {
                problems.push(format!("{at}: {ty}'s field {missing} isn't listed"));
            }
        }
    }
    for ty in shape.fields.keys() {
        if !ty.starts_with("content:") && !documented.contains_key(ty) {
            problems.push(format!(
                "{ty} has no field table (<!-- ir-fields: {ty} -->)"
            ));
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn the_content_node_table_lists_every_node_and_its_fields() {
    let shape = Shape::of(&maximal());
    let text = page("docs/schema/index.qmd");
    let t = tables(&text)
        .into_iter()
        .find(|t| t.marker.as_deref() == Some("ir-content-nodes"))
        .expect("docs/schema/index.qmd: no <!-- ir-content-nodes --> table");
    let mut seen = BTreeSet::new();
    for row in &t.rows {
        let ty = code(&row[0]).unwrap();
        let fields: BTreeSet<String> = code_spans(&row[1]).into_iter().map(|(s, _)| s).collect();
        let mut actual = shape.fields[&format!("content:{ty}")].clone();
        actual.remove("type");
        assert_eq!(fields, actual, "content node {ty}");
        seen.insert(ty);
    }
    assert_eq!(
        seen,
        CONTENT_TYPES.iter().map(|s| s.to_string()).collect(),
        "content node types"
    );
}

// --- Fields named in prose tables ---------------------------------------------------

/// A field as the docs name it (`column_spec[].align`, `config.dir`,
/// `border.*.style`, `image.width`, or a bare style property), resolved to its
/// type and key. A bare name is a field of the name before it in the cell when
/// that type has it (`config.table_width`, `container_width`), else a style's.
fn resolve(shape: &Shape, name: &str, previous: Option<&str>) -> (String, String) {
    let (prefix, key) = match name.rsplit_once('.') {
        Some((p, k)) => (Some(p), k),
        None => (None, name),
    };
    let ty = match prefix {
        None => previous
            .filter(|t| shape.fields[*t].contains(key))
            .unwrap_or("StyleDef"),
        Some("column_spec[]") => "ColumnSpec",
        Some("config") => "Config",
        Some("row") => "Row",
        Some("cell") => "Cell",
        Some("border.*") => "Border",
        Some("padding") => "Padding",
        Some("selector") => "ConditionalSelector",
        Some("typed_value") => "TypedValue",
        Some("image") => "content:image",
        Some(p) => panic!("can't resolve `{name}` (prefix {p})"),
    };
    (ty.to_string(), key.to_string())
}

/// The fields a table cell names.
fn fields_in(shape: &Shape, cell: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (name, _) in code_spans(cell) {
        let previous = out.last().map(|(t, _)| t.clone());
        if name == "padding.*" {
            for side in ["top", "right", "bottom", "left"] {
                out.push(("Padding".into(), side.into()));
            }
            continue;
        }
        out.push(resolve(shape, &name, previous.as_deref()));
    }
    out
}

/// Every place in `maximal.json` a field of this type and key is, by pointer.
fn places(shape: &Shape, ty: &str, key: &str) -> Vec<String> {
    if !shape.fields.get(ty).is_some_and(|f| f.contains(key)) {
        return Vec::new();
    }
    shape
        .objects
        .iter()
        .filter(|(_, _, t)| t == ty)
        .map(|(_, p, _)| format!("{p}/{key}"))
        .collect()
}

/// The rules validation reports with the value at `pointer` replaced.
fn rules_with(max: &Value, pointer: &str, value: Value) -> Option<Vec<ValidationRule>> {
    let mut v = max.clone();
    *v.pointer_mut(pointer).unwrap() = value;
    let table = Table::from_json(&v.to_string()).ok()?;
    Some(table.validate().into_iter().map(|e| e.rule).collect())
}

/// The scalar fields of every type that this rule checks: fields where a nonsense
/// string is reported under it.
fn fields_checked_by(rule: ValidationRule) -> BTreeSet<(String, String)> {
    let max = maximal();
    let shape = Shape::of(&max);
    let mut out = BTreeSet::new();
    for (ty, (pointer, _)) in &shape.first {
        let obj = max.pointer(pointer).unwrap().as_object().unwrap();
        for (key, value) in obj {
            if value.is_object()
                || value.is_array()
                || (ty.starts_with("content:") && key == "type")
            {
                continue;
            }
            let p = format!("{pointer}/{key}");
            if rules_with(&max, &p, json!("zz~zz")).is_some_and(|r| r.contains(&rule)) {
                out.insert((ty.clone(), key.clone()));
            }
        }
    }
    out
}

fn section<'a>(text: &'a str, heading: &str) -> &'a str {
    let start = text
        .find(&format!("\n{heading}\n"))
        .unwrap_or_else(|| panic!("no heading {heading}"));
    let rest = &text[start + heading.len() + 2..];
    let level = heading.split(' ').next().unwrap();
    let end = rest
        .match_indices("\n#")
        .find(|(i, _)| {
            let h = rest[i + 1..].split(' ').next().unwrap();
            h.len() <= level.len() && h.chars().all(|c| c == '#')
        })
        .map_or(rest.len(), |(i, _)| i);
    &rest[..end]
}

type KeywordValues = (
    &'static [&'static str],
    &'static [(&'static str, &'static str)],
);

fn kw<K: Keyword>() -> KeywordValues {
    (K::ALLOWED, K::ALIASES)
}

/// The keyword type of a field.
fn keyword_of(ty: &str, key: &str) -> KeywordValues {
    match (ty, key) {
        ("ColumnSpec", "align") | ("StyleDef", "text_align") => kw::<HAlign>(),
        ("StyleDef", "vertical_align") => kw::<VAlign>(),
        ("Row", "role") => kw::<RowRole>(),
        ("Cell", "scope") => kw::<CellScope>(),
        ("Config", "page_break_mode") => kw::<PageBreakMode>(),
        ("Config", "dir") => kw::<TextDirection>(),
        ("Border", "style") => kw::<BorderStyle>(),
        ("StyleDef", "font_weight") => kw::<FontWeight>(),
        ("StyleDef", "font_style") => kw::<FontStyle>(),
        ("StyleDef", "text_transform") => kw::<TextTransform>(),
        ("StyleDef", "text_decoration") => kw::<TextDecoration>(),
        ("StyleDef", "white_space") => kw::<WhiteSpace>(),
        ("StyleDef", "word_break") => kw::<WordBreak>(),
        ("StyleDef", "overflow") | ("Config", "container_overflow") => kw::<Overflow>(),
        ("StyleDef", "text_overflow") => kw::<TextOverflow>(),
        ("ConditionalSelector", "row_parity") => kw::<RowParity>(),
        ("ConditionalSelector", "scope") => kw::<SelectorScope>(),
        ("TypedValue", "type") | ("Cell", "data_type") => kw::<ValueType>(),
        _ => panic!("{ty}.{key} isn't a keyword field this test knows"),
    }
}

#[test]
fn the_keyword_table_lists_every_keyword_field_and_its_values() {
    let max = maximal();
    let shape = Shape::of(&max);
    let text = page("docs/schema/validation.qmd");
    let section = section(&text, "## Keyword values");
    let t = tables(section).into_iter().next().unwrap();
    let mut documented = BTreeSet::new();
    let mut problems = Vec::new();
    for row in &t.rows {
        let fields = fields_in(&shape, &row[0]);
        let mut values = Vec::new();
        let mut aliases = Vec::new();
        for (s, in_parens) in code_spans(&row[1]) {
            if in_parens {
                aliases.push((s, values.last().cloned().unwrap()));
            } else {
                values.push(s);
            }
        }
        for (ty, key) in &fields {
            documented.insert((ty.clone(), key.clone()));
            let (allowed, known_aliases) = keyword_of(ty, key);
            let allowed: Vec<&str> = allowed.iter().copied().filter(|a| *a != "1–1000").collect();
            if values != allowed {
                problems.push(format!(
                    "{ty}.{key}: documented {values:?}, allowed {allowed:?}"
                ));
            }
            let known: Vec<(String, String)> = known_aliases
                .iter()
                .map(|(a, c)| (a.to_string(), c.to_string()))
                .collect();
            if aliases != known {
                problems.push(format!(
                    "{ty}.{key}: documented aliases {aliases:?}, accepted {known:?}"
                ));
            }
            // And the values do what the table says, in the IR.
            for pointer in places(&shape, ty, key).into_iter().take(1) {
                for v in &values {
                    let rules = rules_with(&max, &pointer, json!(v)).unwrap();
                    if rules.contains(&ValidationRule::UnknownValue) {
                        problems.push(format!("{ty}.{key}: {v} is reported unknown"));
                    }
                }
            }
        }
    }
    let checked = fields_checked_by(ValidationRule::UnknownValue);
    for (ty, key) in checked.difference(&documented) {
        problems.push(format!("the keyword field {ty}.{key} isn't in the table"));
    }
    for (ty, key) in documented.difference(&checked) {
        problems.push(format!(
            "{ty}.{key} is in the table but isn't a keyword field"
        ));
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn the_color_fields_are_the_ones_checked() {
    let text = page("docs/schema/validation.qmd");
    let section = section(&text, "## Colors");
    let first = section.trim_start().split("\n\n").next().unwrap();
    let documented: BTreeSet<_> = fields_in(&Shape::of(&maximal()), first)
        .into_iter()
        .collect();
    assert_eq!(documented, fields_checked_by(ValidationRule::InvalidColor));
}

#[test]
fn the_length_table_lists_every_length_field_and_its_forms() {
    let max = maximal();
    let shape = Shape::of(&max);
    let text = page("docs/schema/validation.qmd");
    let section = section(&text, "## Lengths");
    let t = tables(section).into_iter().next().unwrap();
    assert_eq!(
        t.header,
        ["Field", "Units", "`%`", "`fr`", "`auto`", "Negative"]
    );
    let mut documented = BTreeSet::new();
    let mut problems = Vec::new();
    for row in &t.rows {
        let forms = [
            ("12px", &row[1]),
            ("50%", &row[2]),
            ("2fr", &row[3]),
            ("auto", &row[4]),
            ("-3pt", &row[5]),
        ];
        for (ty, key) in fields_in(&shape, &row[0]) {
            documented.insert((ty.clone(), key.clone()));
            let Some(pointer) = places(&shape, &ty, &key).into_iter().next() else {
                problems.push(format!("no field {ty}.{key}"));
                continue;
            };
            for (value, cell) in forms {
                let ok = !rules_with(&max, &pointer, json!(value))
                    .unwrap()
                    .contains(&ValidationRule::InvalidLength);
                let said = cell.contains('✓');
                if ok != said {
                    problems.push(format!(
                        "{ty}.{key}: {value} is {}, documented {}",
                        if ok { "valid" } else { "invalid" },
                        if said { "✓" } else { "blank" }
                    ));
                }
            }
        }
    }
    let checked = fields_checked_by(ValidationRule::InvalidLength);
    for (ty, key) in checked.difference(&documented) {
        problems.push(format!("the length field {ty}.{key} isn't in the table"));
    }
    for (ty, key) in documented.difference(&checked) {
        problems.push(format!(
            "{ty}.{key} is in the table but isn't a length field"
        ));
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

// --- Rules and messages -------------------------------------------------------------

#[test]
fn the_rule_tables_name_every_rule_once() {
    let text = page("docs/schema/validation.qmd");
    let mut named = Vec::new();
    for t in tables(&text) {
        if t.header[0] == "Rule" {
            named.extend(t.rows.iter().map(|r| code(&r[0]).unwrap()));
        }
    }
    let mut sorted = named.clone();
    sorted.sort();
    let mut all: Vec<String> = ValidationRule::ALL
        .iter()
        .map(|r| r.id().to_string())
        .collect();
    all.sort();
    assert_eq!(sorted, all, "the rule tables list {named:?}");
}

/// A small valid table to break.
fn base() -> Value {
    let cell = |s: &str| json!({ "content": [{ "type": "text", "value": s }] });
    json!({
        "ir_version": "1.0",
        "config": { "table_cols": 3, "header_rows": 1, "body_rows": 2, "stub_cols": 1 },
        "styles": { "defs": { "b": { "border": { "top": { "width": "1px", "style": "double", "color": "black" } } } } },
        "column_spec": [{ "id": "a" }, { "id": "b" }, { "id": "c" }],
        "table": {
            "thead": { "rows": [{ "role": "column_label", "cells": [cell("A"), cell("B"), cell("C")] }] },
            "tbody": [{ "rows": [
                { "cells": [cell("x"), cell("1"), cell("2")] },
                { "cells": [cell("y"), cell("3"), cell("4")] }
            ] }]
        }
    })
}

/// What the validator and the warnings say for each broken or unusual table,
/// one line each, as the docs show them.
fn messages() -> BTreeSet<String> {
    type Edit = fn(&mut Value);
    let edits: &[Edit] = &[
        |v| {
            v["table"]["tbody"][0]["rows"][0]["cells"]
                .as_array_mut()
                .unwrap()
                .push(json!({ "content": [] }))
        },
        |v| v["table"]["tbody"][0]["rows"][1]["cells"][2]["colspan"] = json!(2),
        |v| {
            let cells = &mut v["table"]["tbody"][0]["rows"][0]["cells"];
            cells[1]["colspan"] = json!(2);
            cells[2]["is_placeholder"] = json!(true);
        },
        |v| v["column_spec"][1]["width"] = json!("-5px"),
        |v| v["config"]["lang"] = json!("fr_CA"),
        |v| v["ir_version"] = json!("2.0"),
        |v| {
            v["ir_version"] = json!("1.3");
            v["column_spec"][0]["align"] = json!("diagonal");
        },
        |v| {
            for i in 0..3 {
                v["table"]["thead"]["rows"][0]["cells"][i]["style_id"] = json!("b");
            }
        },
        |v| v["ir_version"] = json!("1.3"),
        |v| v["table"]["tbody"][0]["rows"][1]["cells"][0]["style_id"] = json!("bold"),
        |v| {
            let note = json!({ "id": "n", "mark": "1", "content": [] });
            v["footer"] = json!({ "footnotes": [note, note] });
        },
    ];
    let mut out = BTreeSet::new();
    for edit in edits {
        let mut v = base();
        edit(&mut v);
        let table = Table::from_json(&v.to_string()).unwrap();
        match table.ensure_valid() {
            Err(invalid) => {
                out.extend(invalid.errors.iter().map(|e| e.to_string()));
                out.extend(invalid.version_note());
            }
            Ok(()) => {
                for w in gridwell_render::warnings(&table, None).unwrap() {
                    out.insert(w.to_string());
                }
            }
        }
    }
    out
}

#[test]
fn the_messages_shown_are_the_ones_written() {
    let known = messages();
    let mut problems = Vec::new();
    let text = page("docs/schema/validation.qmd");
    for b in blocks(&text) {
        if lang(&b.info) != "text" {
            continue;
        }
        for (i, line) in b.body.lines().enumerate() {
            if (line.starts_with('[') || line.starts_with("(the table")) && !known.contains(line) {
                problems.push(format!("validation.qmd:{}: {line}", b.line + 1 + i));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "lines no table writes:\n{}\n\nwritten:\n{}",
        problems.join("\n"),
        known.into_iter().collect::<Vec<_>>().join("\n")
    );
}

/// The JSON pointers to every `key` in `v`.
fn pointers_to(v: &Value, key: &str, at: &str, out: &mut Vec<String>) {
    match v {
        Value::Object(o) => {
            for (k, x) in o {
                let p = format!("{at}/{k}");
                if k == key {
                    out.push(p.clone());
                }
                pointers_to(x, key, &p, out);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                pointers_to(x, key, &format!("{at}/{i}"), out);
            }
        }
        _ => {}
    }
}

/// What messages call the part of the table a pointer into it is in, e.g. `the
/// cell at tbody group 0 summary (row=0, col=1)` for
/// `/table/tbody/0/summary_rows/0/cells/1/…`.
fn part_named(pointer: &str) -> String {
    let s: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
    let rows = |section: String, rest: &[&str]| match rest {
        [r, "cells", c, ..] => format!("the cell at {section} (row={r}, col={c})"),
        [r, ..] => format!("{section} row {r}"),
        _ => panic!("{pointer}"),
    };
    match s.as_slice() {
        ["header", "extra_lines", i, ..] => format!("header.extra_lines[{i}]"),
        ["header", line, ..] => format!("header.{line}"),
        ["column_spec", i, ..] => format!("column_spec[{i}]"),
        ["table", "thead", "rows", rest @ ..] => rows("thead".into(), rest),
        ["table", "tbody", g, "label", ..] => format!("the label of tbody group {g}"),
        ["table", "tbody", g, "rows", rest @ ..] => rows(format!("tbody group {g}"), rest),
        ["table", "tbody", g, "summary_rows", rest @ ..] => {
            rows(format!("tbody group {g} summary"), rest)
        }
        ["footer", notes, i, ..] => format!("footer.{notes}[{i}]"),
        _ => panic!("no part for {pointer}"),
    }
}

/// The errors validation reports for `v`, as (rule, message).
fn errors_of(v: &Value) -> Vec<(ValidationRule, String)> {
    let table = Table::from_json(&v.to_string()).unwrap();
    table
        .validate()
        .into_iter()
        .map(|e| (e.rule, e.message))
        .collect()
}

#[test]
fn every_style_reference_is_checked_and_named() {
    // Every `style_id` and `extends` in maximal.json, and a styled_text node added
    // to every content, naming no style: each is reported once, saying where.
    let max = maximal();
    let mut plants = Vec::new();
    let mut ids = Vec::new();
    pointers_to(&max, "style_id", "", &mut ids);
    for p in ids {
        let mut v = max.clone();
        *v.pointer_mut(&p).unwrap() = json!("zz");
        let want = if p.contains("/content/") {
            format!(
                "style_id \"zz\" of a styled_text node in {} is",
                part_named(&p)
            )
        } else {
            format!("style_id \"zz\" of {} is", part_named(&p))
        };
        plants.push((p, v, want));
    }
    let mut contents = Vec::new();
    pointers_to(&max, "content", "", &mut contents);
    for p in contents {
        let mut v = max.clone();
        let node = json!({ "type": "styled_text", "value": "x", "style_id": "zz" });
        v.pointer_mut(&p)
            .unwrap()
            .as_array_mut()
            .unwrap()
            .push(node);
        let want = format!(
            "style_id \"zz\" of a styled_text node in {} is",
            part_named(&p)
        );
        plants.push((p, v, want));
    }
    let mut extends = Vec::new();
    pointers_to(&max, "extends", "", &mut extends);
    for p in extends {
        // Nothing, and a composition (here, itself).
        for base in ["zz", "c"] {
            let mut v = max.clone();
            *v.pointer_mut(&p).unwrap() = json!(base);
            let want = format!("styles.compositions.c.extends is \"{base}\", which is not");
            plants.push((p.clone(), v, want));
        }
    }
    assert!(plants.len() >= 30, "only {} places", plants.len());
    for (p, v, want) in plants {
        let errors = errors_of(&v);
        assert!(
            matches!(errors.as_slice(), [(ValidationRule::StyleRefsValid, m)] if m.starts_with(&want)),
            "{p}: want one STYLE_REFS_VALID starting {want:?}, got {errors:?}"
        );
    }
}

#[test]
fn every_footnote_reference_is_checked_and_named() {
    // A footnote_mark naming no footnote, added to every content in maximal.json
    // (the notes' too), is reported once, saying where.
    let max = maximal();
    let mut contents = Vec::new();
    pointers_to(&max, "content", "", &mut contents);
    assert!(contents.len() > 10, "only {} contents", contents.len());
    for p in contents {
        let mut v = max.clone();
        let node = json!({ "type": "footnote_mark", "ref": "zz", "mark_text": "z" });
        v.pointer_mut(&p)
            .unwrap()
            .as_array_mut()
            .unwrap()
            .push(node);
        let want = format!("footnote_mark in {} references \"zz\"", part_named(&p));
        let errors = errors_of(&v);
        assert!(
            matches!(errors.as_slice(), [(ValidationRule::FootnoteRefsValid, m)] if m.starts_with(&want)),
            "{p}: want one FOOTNOTE_REFS_VALID starting {want:?}, got {errors:?}"
        );
    }
}

// --- JSON examples ----------------------------------------------------------------

/// An example with `...` for what it leaves out, as JSON: `[...]` is `[]`, and a
/// `...` member or item is dropped.
fn unelide(s: &str) -> String {
    let mut out = s.replace("[...]", "[]").replace("{...}", "{}");
    for pat in [", ...", ",...", "...,", "..."] {
        out = out.replace(pat, "");
    }
    out
}

/// A content node of each type, from `maximal.json`.
fn content_models() -> &'static BTreeMap<String, Value> {
    static MODELS: std::sync::OnceLock<BTreeMap<String, Value>> = std::sync::OnceLock::new();
    MODELS.get_or_init(|| {
        let max = maximal();
        let shape = Shape::of(&max);
        shape
            .first
            .iter()
            .filter_map(|(ty, (pointer, _))| {
                let t = ty.strip_prefix("content:")?;
                Some((t.to_string(), max.pointer(pointer).unwrap().clone()))
            })
            .collect()
    })
}

/// The model for an item of an array: a content node of its type, else the
/// model array's first item.
fn item_model<'a>(item: &Value, model: &'a [Value], path: &str) -> Option<&'a Value> {
    if path.ends_with("content") {
        if let Some(t) = item.get("type").and_then(Value::as_str) {
            return content_models().get(t).or(model.first());
        }
    }
    model.first()
}

/// Every key of `example`, recursively, is in `model` (which has every field): a
/// map's entries are checked against the model's first, an array's items against
/// the model's item of the same `type` (else its first).
fn fits(example: &Value, model: &Value, path: &str) -> Result<(), String> {
    if FREE.iter().any(|p| matches(p, path)) {
        return Ok(());
    }
    match (example, model) {
        (Value::Object(e), Value::Object(m)) if MAPS.contains(&path) => match m.values().next() {
            Some(first) => e
                .values()
                .try_for_each(|v| fits(v, first, &join(path, "*"))),
            None => Ok(()),
        },
        (Value::Object(e), Value::Object(m)) => e.iter().try_for_each(|(k, v)| match m.get(k) {
            Some(mv) => fits(v, mv, &join(path, k)),
            None => Err(format!("`{}` isn't a field", join(path, k))),
        }),
        (Value::Array(e), Value::Array(m)) => e.iter().try_for_each(|item| {
            let model_item = item_model(item, m, path)
                .ok_or_else(|| format!("`{path}` has no items to compare with"))?;
            fits(item, model_item, &format!("{path}[]"))
        }),
        (Value::Object(_) | Value::Array(_), _) if !model.is_null() => {
            Err(format!("`{path}` isn't an object or array in the IR"))
        }
        _ => Ok(()),
    }
}

/// `example` laid over `model`: its fields replace the model's, objects merge, and
/// array items merge over the model's matching item.
fn overlay(example: &Value, model: &Value, path: &str) -> Value {
    if FREE.iter().any(|p| matches(p, path)) {
        return example.clone();
    }
    if path.ends_with("content[]") {
        return example.clone();
    }
    match (example, model) {
        (Value::Object(e), Value::Object(m)) if MAPS.contains(&path) && !m.is_empty() => {
            let first = m.values().next().unwrap();
            Value::Object(
                e.iter()
                    .map(|(k, v)| (k.clone(), overlay(v, first, &join(path, "*"))))
                    .collect(),
            )
        }
        (Value::Object(e), Value::Object(m)) => {
            let mut out = m.clone();
            for (k, v) in e {
                let merged = match m.get(k) {
                    Some(mv) => overlay(v, mv, &join(path, k)),
                    None => v.clone(),
                };
                out.insert(k.clone(), merged);
            }
            Value::Object(out)
        }
        (Value::Array(e), Value::Array(m)) => Value::Array(
            e.iter()
                .map(|item| match item_model(item, m, path) {
                    Some(mi) => overlay(item, mi, &format!("{path}[]")),
                    None => item.clone(),
                })
                .collect(),
        ),
        _ => example.clone(),
    }
}

/// The rules about values (not about how the table fits together), which a part
/// of a table laid over `maximal.json` can still be checked for.
const VALUE_RULES: &[ValidationRule] = &[
    ValidationRule::UnknownValue,
    ValidationRule::InvalidColor,
    ValidationRule::InvalidLength,
    ValidationRule::InvalidStyleId,
    ValidationRule::InvalidLang,
];

/// Checks one JSON example: a whole table must be valid; a part of one must fit
/// somewhere in the IR and hold valid values there.
fn check_example(example: &Value, elided: bool) -> Result<(), String> {
    let max = maximal();
    if example.get("ir_version").is_some() && !elided {
        let table = Table::from_json(&example.to_string()).map_err(|e| e.to_string())?;
        let errors = table.validate();
        if !errors.is_empty() {
            return Err(format!("{errors:?}"));
        }
        let back = serde_json::to_value(&table).unwrap();
        return fits(example, &back, "");
    }
    // A part: try it in place of each object (or array) of the table.
    let shape = Shape::of(&max);
    let mut candidates: Vec<(String, String)> = vec![(String::new(), String::new())];
    candidates.extend(
        shape
            .objects
            .iter()
            .map(|(path, p, _)| (path.clone(), p.clone())),
    );
    candidates.extend(
        MAPS.iter()
            .map(|m| (m.to_string(), format!("/{}", m.replace('.', "/")))),
    );
    let mut why = Vec::new();
    // Where it fits but doesn't parse or holds a bad value.
    let mut failed: Option<String> = None;
    for (path, pointer) in candidates {
        let model = max.pointer(&pointer).unwrap();
        // An array example goes where an array is: the model is its parent's field.
        let (model, path, pointer) = if example.is_array() {
            match pointer.rsplit_once('/') {
                Some((parent, idx)) if idx.parse::<usize>().is_ok() => {
                    let path = path.strip_suffix("[]").unwrap_or(&path).to_string();
                    (max.pointer(parent).unwrap(), path, parent.to_string())
                }
                _ => continue,
            }
        } else {
            (model, path, pointer)
        };
        match fits(example, model, &path) {
            Err(e) => why.push(e),
            Ok(()) => {
                let mut v = max.clone();
                *v.pointer_mut(&pointer).unwrap() = overlay(example, model, &path);
                let table = match Table::from_json(&v.to_string()) {
                    Ok(t) => t,
                    Err(e) => {
                        failed.get_or_insert(format!("at `{path}`: {e}"));
                        continue;
                    }
                };
                let errors: Vec<String> = table
                    .validate()
                    .into_iter()
                    .filter(|e| VALUE_RULES.contains(&e.rule))
                    .map(|e| e.to_string())
                    .collect();
                if errors.is_empty() {
                    return Ok(());
                }
                failed.get_or_insert(format!("at `{path}`: {errors:?}"));
            }
        }
    }
    if let Some(f) = failed {
        return Err(f);
    }
    // The nearest misses, shortest first.
    why.sort_by_key(|w| w.len());
    why.dedup();
    why.truncate(3);
    Err(format!(
        "fits nowhere in the IR, nearest: {}",
        why.join("; ")
    ))
}

#[test]
fn every_json_example_fits_the_ir() {
    let mut problems = Vec::new();
    let mut checked = 0;
    for (rel, text) in pages() {
        for b in blocks(&text) {
            if lang(&b.info) != "json" {
                continue;
            }
            let at = format!("{rel}:{}", b.line);
            let elided = b.body.contains("...");
            let example = match serde_json::from_str::<Value>(&b.body) {
                Ok(v) => v,
                Err(_) if elided => match serde_json::from_str::<Value>(&unelide(&b.body)) {
                    Ok(v) => v,
                    Err(e) => {
                        problems.push(format!("{at}: not JSON even without the `...`: {e}"));
                        continue;
                    }
                },
                Err(e) => {
                    problems.push(format!("{at}: not JSON: {e}"));
                    continue;
                }
            };
            checked += 1;
            if let Err(e) = check_example(&example, elided) {
                problems.push(format!("{at}: {e}"));
            }
        }
    }
    assert!(checked > 15, "only {checked} JSON examples found");
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn the_example_check_catches_mistakes() {
    // A whole table that's invalid, a field that doesn't exist, a bad value.
    let mut bad = base();
    bad["config"]["table_cols"] = json!(4);
    assert!(check_example(&bad, false).is_err());
    assert!(check_example(
        &json!({ "config": { "table_cols": 2, "colour": "red" } }),
        false
    )
    .is_err());
    assert!(check_example(
        &json!({ "column_spec": [{ "id": "a", "align": "middle" }] }),
        false
    )
    .is_err());
    assert!(check_example(&json!({ "font_weight": "heavy" }), false).is_err());
    assert!(check_example(
        &json!([{ "content": [], "colspan": 2, "is_placholder": true }]),
        false
    )
    .is_err());
    // And passes what's right.
    check_example(&base(), false).unwrap();
    check_example(
        &json!({ "column_spec": [{ "id": "a", "align": "right" }] }),
        false,
    )
    .unwrap();
    check_example(
        &json!({ "font_weight": "bold", "border": { "top": { "style": "solid" } } }),
        false,
    )
    .unwrap();
    check_example(
        &json!([{ "content": [], "colspan": 1, "is_placeholder": true }]),
        false,
    )
    .unwrap();
}

// --- Links --------------------------------------------------------------------------

/// The ids Pandoc gives a page's headings (and any `{#id}` set on one).
fn anchors(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (_, line) in prose(text) {
        let Some(title) = line.trim_start_matches('#').strip_prefix(' ') else {
            continue;
        };
        if !line.starts_with('#') {
            continue;
        }
        if let Some((_, id)) = title.split_once("{#") {
            out.insert(id.split(['}', ' ']).next().unwrap().to_string());
            continue;
        }
        let title = title.split(" {").next().unwrap();
        // Pandoc: drop formatting, keep alphanumerics, `_`, `-` and `.`, spaces
        // become `-`, lowercase, and drop everything before the first letter.
        let plain: String = title
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '))
            .map(|c| if c == ' ' { '-' } else { c })
            .flat_map(char::to_lowercase)
            .collect();
        let id = plain
            .trim_start_matches(|c: char| !c.is_alphabetic())
            .to_string();
        let id = if id.is_empty() {
            "section".to_string()
        } else {
            id
        };
        let n = counts.entry(id.clone()).or_default();
        out.insert(if *n == 0 {
            id.clone()
        } else {
            format!("{id}-{n}")
        });
        *n += 1;
    }
    out
}

#[test]
fn every_link_between_pages_lands() {
    let pages = pages();
    let by_path: BTreeMap<&str, &str> = pages
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let mut problems = Vec::new();
    let mut checked = 0;
    for (rel, text) in &pages {
        let dir = Path::new(rel).parent().unwrap();
        for (n, line) in prose(text) {
            let mut rest = line;
            while let Some(i) = rest.find("](") {
                let after = &rest[i + 2..];
                let end = after.find(')').unwrap_or(after.len());
                let target = after[..end].split_whitespace().next().unwrap_or("");
                rest = &after[end..];
                if target.contains("://")
                    || target.starts_with("mailto:")
                    || target.starts_with('/')
                {
                    continue;
                }
                let (file, anchor) = target.split_once('#').unwrap_or((target, ""));
                checked += 1;
                let at = format!("{rel}:{n}: {target}");
                let page = if file.is_empty() {
                    rel.clone()
                } else {
                    // Normalize `a/../b` (`a\..\b` once joined on Windows).
                    let joined = dir.join(file).to_string_lossy().into_owned();
                    let mut parts: Vec<&str> = Vec::new();
                    for part in joined.split(['/', '\\']) {
                        match part {
                            ".." => {
                                parts.pop();
                            }
                            "." | "" => {}
                            p => parts.push(p),
                        }
                    }
                    parts.join("/")
                };
                if !root().join(&page).exists() {
                    problems.push(format!("{at}: no {page}"));
                    continue;
                }
                if anchor.is_empty() {
                    continue;
                }
                match by_path.get(page.as_str()) {
                    Some(t) if anchors(t).contains(anchor) => {}
                    Some(_) => problems.push(format!("{at}: no heading #{anchor} in {page}")),
                    None => problems.push(format!("{at}: {page} isn't a page this test reads")),
                }
            }
        }
    }
    assert!(checked > 50, "only {checked} links found");
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn every_page_is_in_the_site() {
    let site = read("docs/_quarto.yml");
    for (rel, _) in pages() {
        let Some(page) = rel.strip_prefix("docs/") else {
            continue;
        };
        if page == "index.qmd" {
            continue;
        }
        assert!(site.contains(page), "{rel} isn't in docs/_quarto.yml");
    }
}

// --- Experimental formats ------------------------------------------------------

/// The registry name of a format as the docs name it ("LaTeX", "ANSI (Terminal)",
/// "PowerPoint", "`pptx`").
fn format_named(name: &str) -> String {
    let first = name.split_whitespace().next().unwrap_or("");
    let first = first.trim_matches('`').to_lowercase();
    match first.as_str() {
        "word" => "docx".into(),
        "excel" => "xlsx".into(),
        "powerpoint" => "pptx".into(),
        _ => first,
    }
}

/// The formats a sentence lists before `marker` ("SVG, ANSI and PPTX are …").
fn listed_before(text: &str, marker: &str, rel: &str) -> Vec<String> {
    let at = text
        .find(marker)
        .unwrap_or_else(|| panic!("{rel}: no \"{marker}\""));
    let start = text[..at].rfind("\n\n").map_or(0, |i| i + 2);
    text[start..at]
        .split(", ")
        .flat_map(|s| s.split(" and "))
        .map(format_named)
        .collect()
}

#[test]
fn the_docs_name_the_experimental_formats_the_registry_marks() {
    let all: Vec<String> = gridwell_render::REGISTRY
        .iter()
        .map(|w| w.name().to_string())
        .collect();
    let experimental: Vec<String> = gridwell_render::REGISTRY
        .iter()
        .filter(|w| w.experimental())
        .map(|w| w.name().to_string())
        .collect();
    assert!(!experimental.is_empty());

    // The home page's format table: a status for every format.
    let index = page("docs/index.qmd");
    let table = tables(&index)
        .into_iter()
        .find(|t| t.header.first().is_some_and(|h| h == "Format"))
        .expect("docs/index.qmd: no format table");
    let status = table.header.iter().position(|h| h == "Status").unwrap();
    let names: Vec<String> = table.rows.iter().map(|r| format_named(&r[0])).collect();
    assert_eq!(names, all, "docs/index.qmd: the formats");
    let marked: Vec<String> = table
        .rows
        .iter()
        .filter(|r| {
            assert!(
                ["Stable", "Experimental"].contains(&r[status].as_str()),
                "{r:?}"
            );
            r[status] == "Experimental"
        })
        .map(|r| format_named(&r[0]))
        .collect();
    assert_eq!(marked, experimental, "docs/index.qmd: the Status column");

    // Output Formats: the list, and a note opening each experimental format's
    // section, and no other.
    let rel = "docs/guide/output-formats.qmd";
    let guide = page(rel);
    assert_eq!(
        listed_before(&guide, " are **experimental**", rel),
        experimental,
        "{rel}"
    );
    let words = [
        "no", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        "eleven", "twelve",
    ];
    let stable = format!(
        "The other {} formats are stable.",
        words[all.len() - experimental.len()]
    );
    assert!(guide.contains(&stable), "{rel}: no \"{stable}\"");
    let mut sections = Vec::new();
    let mut marked = Vec::new();
    for group in ["## Text Formats", "## Binary Formats"] {
        let body = section(&guide, group);
        for part in body.split("\n### ").skip(1) {
            let (heading, rest) = part.split_once('\n').unwrap();
            let name = format_named(heading);
            if rest.trim_start().starts_with("*Experimental:") {
                marked.push(name.clone());
            }
            sections.push(name);
        }
    }
    sections.sort();
    let mut sorted = all.clone();
    sorted.sort();
    assert_eq!(sections, sorted, "{rel}: a section for every format");
    assert_eq!(
        marked, experimental,
        "{rel}: the sections marked experimental"
    );

    // The READMEs.
    for rel in ["README.md", "crates/gridwell/README.md"] {
        assert_eq!(
            listed_before(&page(rel), " are experimental for now", rel),
            experimental,
            "{rel}"
        );
    }
}
