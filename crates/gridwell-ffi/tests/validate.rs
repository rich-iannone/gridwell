//! `gridwell_validate`: every validation error, one per line, as the Rust API
//! reports them.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::PathBuf;
use std::ptr;

use gridwell_ffi::*;
use serde_json::{json, Value};

fn fixture(rel: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(rel);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

/// What `gridwell_validate` says of `json`: None if valid, else the error's code
/// and message. Frees what it gets back.
fn validate(json: &str) -> Option<(i32, String)> {
    let mut err = ptr::null_mut();
    let table = unsafe { gridwell_parse_ir(json.as_ptr() as *const c_char, json.len(), &mut err) };
    assert!(!table.is_null() && err.is_null(), "parse failed");
    let err = unsafe { gridwell_validate(table) };
    unsafe { gridwell_free_table(table) };
    if err.is_null() {
        return None;
    }
    let code = unsafe { gridwell_error_code(err) };
    let msg = unsafe { CStr::from_ptr(gridwell_error_message(err)) }
        .to_str()
        .unwrap()
        .to_owned();
    unsafe { gridwell_free_error(err) };
    Some((code, msg))
}

/// The table's validation errors as the Rust API reports them.
fn expected(json: &str) -> Vec<String> {
    let table = gridwell_ir::Table::from_json(json).unwrap();
    table.validate().iter().map(|e| e.to_string()).collect()
}

/// A table with 12 footnotes of one id: 11 errors, more than a rendering error
/// lists.
fn many_errors() -> Value {
    let mut ir = fixture("minimal/minimal_1x1.json");
    let note = json!({ "id": "n", "mark": "1", "content": [] });
    ir["footer"]["footnotes"] = Value::Array(vec![note; 12]);
    ir
}

#[test]
fn every_error_is_a_line() {
    let json = many_errors().to_string();
    let (code, msg) = validate(&json).expect("invalid");
    assert_eq!(code, GRIDWELL_ERR_VALIDATE);
    let want = expected(&json);
    assert_eq!(want.len(), 11);
    assert!(gridwell_ir::InvalidTable::DISPLAY_LIMIT < want.len());
    assert_eq!(msg.split('\n').collect::<Vec<_>>(), want);
}

#[test]
fn a_newer_minor_version_ends_with_a_note() {
    let mut ir = many_errors();
    ir["ir_version"] = json!("1.2");
    let json = ir.to_string();
    let (_, msg) = validate(&json).expect("invalid");
    let lines: Vec<&str> = msg.split('\n').collect();
    let want = expected(&json);
    assert_eq!(lines.len(), want.len() + 1, "{msg}");
    assert_eq!(lines[..want.len()], want);
    let note = gridwell_ir::Table::from_json(&json)
        .unwrap()
        .ensure_valid()
        .unwrap_err()
        .version_note()
        .unwrap();
    assert_eq!(lines[want.len()], note);
    assert!(note.contains("IR 1.2, newer than"), "{note}");
    // The current version has no note.
    let (_, msg) = validate(&many_errors().to_string()).unwrap();
    assert!(!msg.contains("newer than"), "{msg}");
}

#[test]
fn a_valid_table_is_null_whatever_its_minor_version() {
    let mut ir = fixture("minimal/minimal_1x1.json");
    assert_eq!(validate(&ir.to_string()), None);
    ir["ir_version"] = json!("1.2");
    assert_eq!(validate(&ir.to_string()), None);
}
