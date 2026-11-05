//! `gridwell_warnings`: what a format can't express, one message per line.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::ptr;

use gridwell_ffi::*;

fn fixture(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn parse(json: &str) -> *mut GridwellTable {
    let mut err = ptr::null_mut();
    let table = unsafe { gridwell_parse_ir(json.as_ptr() as *const c_char, json.len(), &mut err) };
    assert!(!table.is_null() && err.is_null(), "parse failed");
    table
}

/// The warnings for `format` (null for every format), or the error's code and
/// message. Frees what it gets back.
fn warnings(table: *const GridwellTable, format: Option<&str>) -> Result<String, (i32, String)> {
    let name = format.map(|f| CString::new(f).unwrap());
    let name = name.as_ref().map_or(ptr::null(), |n| n.as_ptr());
    let mut err = ptr::null_mut();
    let r = unsafe { gridwell_warnings(table, name, &mut err) };
    if r.text.is_null() {
        assert_eq!(r.len, 0);
        assert!(!err.is_null(), "failed without an error");
        let code = unsafe { gridwell_error_code(err) };
        let msg = unsafe { CStr::from_ptr(gridwell_error_message(err)) }
            .to_string_lossy()
            .into_owned();
        unsafe { gridwell_free_error(err) };
        return Err((code, msg));
    }
    assert!(err.is_null(), "succeeded with an error");
    let text = unsafe { CStr::from_ptr(r.text) }
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(text.len(), r.len, "len excludes the terminator");
    unsafe { gridwell_free_text_result(r) };
    Ok(text)
}

/// What the Rust API says, as the FFI should write it.
fn expected(json: &str, format: Option<&str>) -> String {
    let table = gridwell_ir::Table::from_json(json).unwrap();
    let found = gridwell_render::warnings(&table, format).unwrap();
    found
        .iter()
        .map(|w| w.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn warnings_are_one_per_line_for_a_format_or_all() {
    let json = fixture("minimal/styles_borders.json");
    let table = parse(&json);

    let latex = warnings(table, Some("latex")).unwrap();
    assert!(
        latex.starts_with(
            "[UNSUPPORTED_FEATURE] latex can't express border-styles (group 1 row 2, column 1"
        ),
        "{latex}"
    );
    assert_eq!(latex.lines().count(), 1, "{latex}");
    assert_eq!(latex, expected(&json, Some("latex")));
    assert_eq!(warnings(table, Some(" LaTeX ")).unwrap(), latex);

    // Null is every format, in registry order; no trailing newline.
    let every = warnings(table, None).unwrap();
    assert_eq!(every, expected(&json, None));
    let lines: Vec<&str> = every.split('\n').collect();
    assert_eq!(lines.len(), 2, "{every}");
    assert_eq!(lines[0], latex);
    assert!(
        lines[1].starts_with("[UNSUPPORTED_FEATURE] typst "),
        "{every}"
    );

    unsafe { gridwell_free_table(table) };
}

#[test]
fn no_warnings_is_an_empty_string_not_a_failure() {
    let table = parse(&fixture("minimal/styles_borders.json"));
    assert_eq!(warnings(table, Some("html")).unwrap(), "");
    unsafe { gridwell_free_table(table) };
    let table = parse(&fixture("minimal/minimal_1x1.json"));
    assert_eq!(warnings(table, None).unwrap(), "");
    unsafe { gridwell_free_table(table) };
}

#[test]
fn every_corpus_table_warns_as_the_rust_api_does() {
    for example in gridwell_testkit::examples() {
        let json = example.table().to_json().unwrap();
        let table = parse(&json);
        assert_eq!(
            warnings(table, None).unwrap(),
            expected(&json, None),
            "{}",
            example.name
        );
        unsafe { gridwell_free_table(table) };
    }
}

#[test]
fn warnings_check_the_table_and_the_format() {
    let invalid = parse(&fixture("invalid/span_overlap.json"));
    for format in [Some("latex"), None] {
        let (code, msg) = warnings(invalid, format).unwrap_err();
        assert_eq!(code, GRIDWELL_ERR_VALIDATE, "{format:?}: {msg}");
        assert!(msg.contains("[SPAN_OVERLAP]"), "{format:?}: {msg}");
    }
    unsafe { gridwell_free_table(invalid) };

    let table = parse(&fixture("minimal/styles_borders.json"));
    let (code, msg) = warnings(table, Some("pdf")).unwrap_err();
    assert_eq!(code, GRIDWELL_ERR_RENDER);
    assert!(msg.contains("unknown format \"pdf\""), "{msg}");

    // Text and binary formats alike (DOCX has none here).
    assert_eq!(warnings(table, Some("docx")).unwrap(), "");

    let mut err = ptr::null_mut();
    let bad = [0xffu8, 0];
    let r = unsafe { gridwell_warnings(table, bad.as_ptr() as *const c_char, &mut err) };
    assert!(r.text.is_null());
    assert_eq!(
        unsafe { gridwell_error_code(err) },
        GRIDWELL_ERR_INVALID_ARG
    );
    unsafe { gridwell_free_error(err) };

    let (code, _) = warnings(ptr::null(), None).unwrap_err();
    assert_eq!(code, GRIDWELL_ERR_INVALID_ARG);

    // A null err out-pointer is allowed.
    let fmt = CString::new("pdf").unwrap();
    let r = unsafe { gridwell_warnings(table, fmt.as_ptr(), ptr::null_mut()) };
    assert!(r.text.is_null());
    unsafe { gridwell_free_table(table) };
}

#[test]
fn a_newer_minor_version_is_the_first_warning() {
    let mut ir: serde_json::Value =
        serde_json::from_str(&fixture("minimal/styles_borders.json")).unwrap();
    ir["ir_version"] = serde_json::json!("1.2");
    let json = ir.to_string();
    let table = parse(&json);
    let every = warnings(table, None).unwrap();
    assert_eq!(every, expected(&json, None));
    let lines: Vec<&str> = every.split('\n').collect();
    assert_eq!(lines.len(), 3, "{every}");
    assert!(
        lines[0].starts_with("[NEWER_IR_VERSION] the table is IR 1.2, newer than"),
        "{every}"
    );
    assert!(
        lines[1].starts_with("[UNSUPPORTED_FEATURE] latex "),
        "{every}"
    );
    assert_eq!(warnings(table, Some("html")).unwrap(), lines[0]);
    unsafe { gridwell_free_table(table) };
}

#[test]
fn another_major_version_fails_to_parse_or_validate() {
    let mut ir: serde_json::Value =
        serde_json::from_str(&fixture("minimal/minimal_1x1.json")).unwrap();
    ir["ir_version"] = serde_json::json!("2.0");
    // Parses as IR 1: invalid.
    let table = parse(&ir.to_string());
    let (code, msg) = warnings(table, None).unwrap_err();
    assert_eq!(code, GRIDWELL_ERR_VALIDATE, "{msg}");
    assert!(
        msg.contains("[IR_VERSION] ir_version is \"2.0\", IR 2"),
        "{msg}"
    );
    unsafe { gridwell_free_table(table) };
    // Doesn't: a parse error that says why.
    ir["table"] = serde_json::json!({ "sections": [] });
    let json = ir.to_string();
    let mut err = ptr::null_mut();
    let table = unsafe { gridwell_parse_ir(json.as_ptr() as *const c_char, json.len(), &mut err) };
    assert!(table.is_null() && !err.is_null());
    assert_eq!(unsafe { gridwell_error_code(err) }, GRIDWELL_ERR_PARSE);
    let msg = unsafe { CStr::from_ptr(gridwell_error_message(err)) }
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        msg,
        "ir_version is \"2.0\", IR 2, and this gridwell reads only IR 1.x (a newer gridwell \
         may read it)"
    );
    unsafe { gridwell_free_error(err) };
}
