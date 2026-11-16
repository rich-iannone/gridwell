//! IR 1 stays readable. The tables in `compat/ir-1.0/` were written for IR 1.0 (by
//! the example corpus, the fixtures, the docs and the gt and Great Tables
//! emitters), and every later gridwell that reads IR 1 must still read them the
//! same: each parses, validates, renders in every format, writes back to IR that
//! reads the same, and keeps its committed normalized grid (the conformance kit's
//! L3 grid: structure, content and resolved styling). `MANIFEST` pins each file's
//! bytes, so a frozen table can't be edited to make a test pass.
//!
//! A newer minor IR version may only add to IR 1, so a table must also read the
//! same with a newer minor's additions in it: an unknown field on any object, an
//! unknown node in any content, a newer `ir_version`. (A conditional style whose
//! selector has an unknown field applies to no row: a newer minor may narrow a
//! selector with it.)
//!
//! `GRIDWELL_BLESS_COMPAT=1` writes the grid and the `MANIFEST` line of a table
//! that has none yet. It never changes an existing one: delete it first, and say
//! why in the commit (see `compat/README.md`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Once;

use gridwell_ir::Table;
use gridwell_render::conformance::{check, normalize, Tier};
use gridwell_render::{names, render, Output};
use serde_json::{json, Value};

/// The IR versions with a compatibility corpus, oldest first.
const VERSIONS: &[&str] = &["1.0"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn bless() -> bool {
    std::env::var_os("GRIDWELL_BLESS_COMPAT").is_some_and(|v| !v.is_empty())
}

/// With `GRIDWELL_BLESS_COMPAT`, write the grid of every table that has none,
/// once, before any test reads them.
fn bless_missing_grids() {
    static ONCE: Once = Once::new();
    if !bless() {
        return;
    }
    ONCE.call_once(|| {
        for version in VERSIONS {
            for path in tables(version) {
                let grid = grid_path(&path);
                if !grid.exists() {
                    if let Ok(v) = parse(&path).validated() {
                        std::fs::write(&grid, pretty(&normalize(&v, Tier::L3))).unwrap();
                    }
                }
            }
        }
    });
}

fn dir(version: &str) -> PathBuf {
    root().join("compat").join(format!("ir-{version}"))
}

/// A file's path from its version's directory, with `/`.
fn rel(version: &str, path: &Path) -> String {
    let rel = path.strip_prefix(dir(version)).unwrap();
    rel.to_string_lossy().replace('\\', "/")
}

/// Every table of a version (not the grids), sorted.
fn tables(version: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dirs = vec![dir(version)];
    while let Some(d) = dirs.pop() {
        for entry in std::fs::read_dir(&d).unwrap_or_else(|e| panic!("{}: {e}", d.display())) {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if path.is_dir() {
                dirs.push(path);
            } else if name.ends_with(".json") && !name.ends_with(".grid.json") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn grid_path(table: &Path) -> PathBuf {
    let name = table.file_name().unwrap().to_string_lossy();
    table.with_file_name(name.replace(".json", ".grid.json"))
}

/// The file as text, with CRLF read as LF (a Windows checkout may have either).
fn text(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n")
}

fn parse(path: &Path) -> Table {
    Table::from_json(&text(path)).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap() + "\n"
}

/// 64-bit FNV-1a, enough to notice an edit.
fn fnv1a(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// `MANIFEST`: file → hash.
fn manifest(version: &str) -> BTreeMap<String, String> {
    let path = dir(version).join("MANIFEST");
    let text = if path.exists() {
        text(&path)
    } else {
        String::new()
    };
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (hash, file) = l
                .split_once("  ")
                .unwrap_or_else(|| panic!("MANIFEST: not `hash  file`: {l}"));
            (file.to_string(), hash.to_string())
        })
        .collect()
}

fn write_manifest(version: &str, entries: &BTreeMap<String, String>) {
    let mut out = String::from(
        "# The FNV-1a hash of every frozen file (LF line ends). See compat/README.md.\n",
    );
    for (file, hash) in entries {
        out.push_str(&format!("{hash}  {file}\n"));
    }
    std::fs::write(dir(version).join("MANIFEST"), out).unwrap();
}

#[test]
fn every_frozen_file_is_unchanged() {
    bless_missing_grids();
    let mut problems = Vec::new();
    for version in VERSIONS {
        let mut entries = manifest(version);
        let mut files: Vec<PathBuf> = Vec::new();
        for table in tables(version) {
            files.push(grid_path(&table));
            files.push(table);
        }
        let mut added = false;
        for file in &files {
            if !file.exists() {
                continue; // a grid not written yet: the grid test says so
            }
            let name = rel(version, file);
            let hash = fnv1a(&text(file));
            match entries.get(&name) {
                Some(h) if *h == hash => {}
                Some(_) => problems.push(format!(
                    "ir-{version}/{name} has changed: a frozen file is never edited"
                )),
                None if bless() => {
                    entries.insert(name, hash);
                    added = true;
                }
                None => problems.push(format!(
                    "ir-{version}/{name} isn't in MANIFEST (GRIDWELL_BLESS_COMPAT=1 adds it)"
                )),
            }
        }
        for name in entries.keys() {
            if !dir(version).join(name).exists() {
                problems.push(format!(
                    "ir-{version}/{name} is in MANIFEST but gone: a frozen file is never removed"
                ));
            }
        }
        if added {
            write_manifest(version, &entries);
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn every_table_reads_as_its_version_says() {
    for version in VERSIONS {
        for path in tables(version) {
            let table = parse(&path);
            assert_eq!(table.ir_version, *version, "{}", path.display());
            assert!(!table.is_newer_minor(), "{}", path.display());
        }
    }
}

#[test]
fn every_table_validates_and_keeps_its_grid() {
    bless_missing_grids();
    let mut problems = Vec::new();
    for version in VERSIONS {
        for path in tables(version) {
            let name = format!("ir-{version}/{}", rel(version, &path));
            let table = parse(&path);
            if let Err(e) = table.validated() {
                problems.push(format!("{name} is invalid: {e}"));
                continue;
            }
            let grid = grid_path(&path);
            if !grid.exists() {
                problems.push(format!(
                    "{name} has no grid (GRIDWELL_BLESS_COMPAT=1 writes it)"
                ));
                continue;
            }
            let expected: Value = serde_json::from_str(&text(&grid)).unwrap();
            let report = check(&table, &expected, Some(Tier::L3)).unwrap();
            if !report.passed() {
                problems.push(format!("{name} reads differently now:\n{report}"));
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn every_table_renders_in_every_format() {
    let mut problems = Vec::new();
    for version in VERSIONS {
        for path in tables(version) {
            let table = parse(&path);
            for format in names() {
                match render(&table, format, None).map(Output::into_bytes) {
                    Ok(bytes) if !bytes.is_empty() => {}
                    Ok(_) => problems.push(format!("{}: empty {format}", path.display())),
                    Err(e) => problems.push(format!("{}: {format}: {e}", path.display())),
                }
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// The grid of a table, which must be valid.
fn grid_of(table: &Table) -> Value {
    normalize(&table.validated().unwrap(), Tier::L3)
}

#[test]
fn every_table_writes_back_to_ir_that_reads_the_same() {
    for version in VERSIONS {
        for path in tables(version) {
            let table = parse(&path);
            let json = table.to_json().unwrap();
            let again = Table::from_json(&json).unwrap();
            assert_eq!(grid_of(&again), grid_of(&table), "{}", path.display());
            // Writing back is stable: the second write is the first.
            assert_eq!(again.to_json().unwrap(), json, "{}", path.display());
        }
    }
}

// --- What a newer minor version may add -----------------------------------------------

/// The JSON pointer of every object in a table, content nodes included. Maps of names (style definitions and compositions) aren't objects of the IR,
/// and free JSON (`extensions`, `preheader_content`, a typed value's `value`) isn't
/// either.
fn objects(v: &Value, pointer: &str, out: &mut Vec<String>) {
    let free = ["/extensions", "/header/preheader_content"];
    if free.contains(&pointer) || pointer.ends_with("/typed_value/value") {
        return;
    }
    match v {
        Value::Object(map) => {
            let is_map = pointer == "/styles/defs" || pointer == "/styles/compositions";
            if !is_map {
                out.push(pointer.to_string());
            }
            for (k, item) in map {
                let k = k.replace('~', "~0").replace('/', "~1");
                objects(item, &format!("{pointer}/{k}"), out);
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                objects(item, &format!("{pointer}/{i}"), out);
            }
        }
        _ => {}
    }
}

/// The pointer of every content list in a table.
fn content_lists(v: &Value, pointer: &str, out: &mut Vec<String>) {
    match v {
        Value::Object(map) => {
            for (k, item) in map {
                let p = format!("{pointer}/{}", k.replace('~', "~0").replace('/', "~1"));
                if k == "content" && item.is_array() {
                    out.push(p.clone());
                }
                if pointer != "/extensions" {
                    content_lists(item, &p, out);
                }
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                content_lists(item, &format!("{pointer}/{i}"), out);
            }
        }
        _ => {}
    }
}

/// What a newer minor might add to an object: a field of every JSON kind.
fn future_fields() -> Vec<(&'static str, Value)> {
    vec![
        ("zz_future_string", json!("value")),
        ("zz_future_number", json!(3.5)),
        ("zz_future_bool", json!(true)),
        ("zz_future_null", Value::Null),
        ("zz_future_array", json!([1, "two", {"three": 3}])),
        ("zz_future_object", json!({"nested": {"deeper": [true]}})),
    ]
}

/// Every format's output for a table.
fn outputs(table: &Table) -> Vec<(&'static str, Vec<u8>)> {
    names()
        .into_iter()
        .map(|f| {
            let out = render(table, f, None).unwrap_or_else(|e| panic!("{f}: {e}"));
            (f, Output::into_bytes(out))
        })
        .collect()
}

#[test]
fn unknown_fields_on_any_object_change_nothing() {
    let mut problems = Vec::new();
    for version in VERSIONS {
        for path in tables(version) {
            let name = format!("ir-{version}/{}", rel(version, &path));
            let original: Value = serde_json::from_str(&text(&path)).unwrap();
            let expected = grid_of(&parse(&path));
            let mut places = Vec::new();
            objects(&original, "", &mut places);
            for pointer in places {
                // A selector with an unknown field reads as no conditional.
                let expected = match pointer.strip_suffix("/selector") {
                    Some(conditional) => {
                        let (list, i) = conditional.rsplit_once('/').unwrap();
                        let mut without = original.clone();
                        let list = without.pointer_mut(list).unwrap().as_array_mut().unwrap();
                        list.remove(i.parse().unwrap());
                        grid_of(&Table::from_json(&without.to_string()).unwrap())
                    }
                    None => expected.clone(),
                };
                let mut v = original.clone();
                let obj = if pointer.is_empty() {
                    &mut v
                } else {
                    v.pointer_mut(&pointer).unwrap()
                };
                for (k, value) in future_fields() {
                    obj.as_object_mut().unwrap().insert(k.into(), value);
                }
                match Table::from_json(&v.to_string()) {
                    Err(e) => problems.push(format!("{name}{pointer}: doesn't parse: {e}")),
                    Ok(t) => match t.validated() {
                        Err(e) => problems.push(format!("{name}{pointer}: invalid: {e}")),
                        Ok(_) if grid_of(&t) != expected => {
                            problems.push(format!("{name}{pointer}: reads differently"))
                        }
                        Ok(_) => {}
                    },
                }
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// The table every format renders with unknown fields everywhere, and with an
/// unknown node in every content list: `docs/maximal.json`, which has every
/// field of every object.
fn maximal() -> (Value, Table) {
    let path = dir("1.0").join("docs/maximal.json");
    (serde_json::from_str(&text(&path)).unwrap(), parse(&path))
}

#[test]
fn unknown_fields_everywhere_render_the_same_in_every_format() {
    let (original, table) = maximal();
    let expected = outputs(&table);
    let mut places = Vec::new();
    objects(&original, "", &mut places);
    let mut v = original.clone();
    for pointer in &places {
        let obj = if pointer.is_empty() {
            &mut v
        } else {
            v.pointer_mut(pointer).unwrap()
        };
        for (k, value) in future_fields() {
            obj.as_object_mut().unwrap().insert(k.into(), value);
        }
    }
    let future = Table::from_json(&v.to_string()).unwrap();
    assert!(future.validate().is_empty());
    for ((format, want), (_, got)) in expected.iter().zip(outputs(&future)) {
        assert!(*want == got, "{format} changed with unknown fields");
    }
}

/// A node of a type a newer minor might add.
fn future_node() -> Value {
    json!({"type": "zz_future_node", "value": "never drawn", "style_id": "nope",
           "content": [{"type": "text", "value": "nor this"}]})
}

#[test]
fn an_unknown_node_in_any_content_is_drawn_as_nothing_and_kept() {
    let (original, table) = maximal();
    let expected = outputs(&table);
    let mut lists = Vec::new();
    content_lists(&original, "", &mut lists);
    assert!(lists.len() > 10, "{lists:?}");
    for pointer in &lists {
        for at in ["start", "end"] {
            let mut v = original.clone();
            let list = v.pointer_mut(pointer).unwrap().as_array_mut().unwrap();
            if list.is_empty() {
                continue; // a placeholder's content stays empty
            }
            match at {
                "start" => list.insert(0, future_node()),
                _ => list.push(future_node()),
            }
            let future = Table::from_json(&v.to_string()).unwrap();
            let errors = future.validate();
            assert!(errors.is_empty(), "{pointer}: {errors:?}");
            for ((format, want), (_, got)) in expected.iter().zip(outputs(&future)) {
                assert!(
                    *want == got,
                    "{format} draws an unknown node at the {at} of {pointer}"
                );
            }
            // Written back whole, where it was.
            let back: Value = serde_json::from_str(&future.to_json().unwrap()).unwrap();
            let list = back.pointer(pointer).unwrap().as_array().unwrap();
            let node = if at == "start" {
                &list[0]
            } else {
                list.last().unwrap()
            };
            assert_eq!(node, &future_node(), "{pointer}");
        }
    }
}

#[test]
fn a_newer_minor_version_reads_the_same_with_a_warning() {
    for version in VERSIONS {
        for path in tables(version) {
            let mut v: Value = serde_json::from_str(&text(&path)).unwrap();
            let table = parse(&path);
            for newer in ["1.1", "1.99"] {
                v["ir_version"] = json!(newer);
                let future = Table::from_json(&v.to_string()).unwrap();
                assert!(future.is_newer_minor());
                assert_eq!(grid_of(&future), grid_of(&table), "{}", path.display());
                let warnings = gridwell_render::warnings(&future, Some("html")).unwrap();
                assert_eq!(
                    warnings.first().map(|w| w.rule()),
                    Some("NEWER_IR_VERSION"),
                    "{}",
                    path.display()
                );
            }
        }
    }
}
