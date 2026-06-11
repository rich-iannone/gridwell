use gridwell_ffi::*;
use std::ffi::CString;
use std::fs;
use std::path::PathBuf;
use std::ptr;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures")
}

fn load_fixture_json(path: &str) -> String {
    let full_path = fixtures_dir().join(path);
    fs::read_to_string(&full_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", full_path.display()))
}

// ─── Parse tests ───

#[test]
fn parse_valid_ir() {
    let json = load_fixture_json("minimal/minimal_1x1.json");
    let mut err: *mut GridwellError = ptr::null_mut();

    let table = unsafe {
        gridwell_parse_ir(json.as_ptr() as *const i8, json.len(), &mut err)
    };

    assert!(!table.is_null(), "parse should succeed");
    assert!(err.is_null(), "no error expected");
    unsafe { gridwell_free_table(table) };
}

#[test]
fn parse_invalid_json() {
    let json = "{ not valid json }}}";
    let mut err: *mut GridwellError = ptr::null_mut();

    let table = unsafe {
        gridwell_parse_ir(json.as_ptr() as *const i8, json.len(), &mut err)
    };

    assert!(table.is_null(), "parse should fail");
    assert!(!err.is_null(), "error expected");

    let code = unsafe { gridwell_error_code(err) };
    assert_eq!(code, 1); // ERR_PARSE

    let msg = unsafe { gridwell_error_message(err) };
    assert!(!msg.is_null());

    unsafe { gridwell_free_error(err) };
}

#[test]
fn parse_null_pointer() {
    let mut err: *mut GridwellError = ptr::null_mut();

    let table = unsafe { gridwell_parse_ir(ptr::null(), 0, &mut err) };

    assert!(table.is_null());
    assert!(!err.is_null());
    let code = unsafe { gridwell_error_code(err) };
    assert_eq!(code, 4); // ERR_INVALID_ARG
    unsafe { gridwell_free_error(err) };
}

// ─── Validate tests ───

#[test]
fn validate_valid_table() {
    let json = load_fixture_json("minimal/minimal_1x1.json");
    let mut err: *mut GridwellError = ptr::null_mut();

    let table = unsafe {
        gridwell_parse_ir(json.as_ptr() as *const i8, json.len(), &mut err)
    };
    assert!(!table.is_null());

    let validation_err = unsafe { gridwell_validate(table) };
    assert!(validation_err.is_null(), "valid table should pass validation");

    unsafe { gridwell_free_table(table) };
}

#[test]
fn validate_invalid_table() {
    let json = load_fixture_json("invalid/col_count_mismatch.json");
    let mut err: *mut GridwellError = ptr::null_mut();

    let table = unsafe {
        gridwell_parse_ir(json.as_ptr() as *const i8, json.len(), &mut err)
    };
    assert!(!table.is_null());

    let validation_err = unsafe { gridwell_validate(table) };
    assert!(!validation_err.is_null(), "invalid table should fail validation");

    let code = unsafe { gridwell_error_code(validation_err) };
    assert_eq!(code, 2); // ERR_VALIDATE

    unsafe { gridwell_free_error(validation_err) };
    unsafe { gridwell_free_table(table) };
}

// ─── Text render tests ───

#[test]
fn render_html() {
    let json = load_fixture_json("minimal/minimal_1x1.json");
    let mut err: *mut GridwellError = ptr::null_mut();

    let table = unsafe {
        gridwell_parse_ir(json.as_ptr() as *const i8, json.len(), &mut err)
    };
    assert!(!table.is_null());

    let format = CString::new("html").unwrap();
    let result = unsafe { gridwell_render_text(table, format.as_ptr(), &mut err) };

    assert!(!result.text.is_null(), "render should produce output");
    assert!(result.len > 0);
    assert!(err.is_null());

    // Check it contains HTML
    let output = unsafe {
        std::str::from_utf8(std::slice::from_raw_parts(result.text as *const u8, result.len))
            .unwrap()
    };
    assert!(output.contains("<table"), "output should be HTML");

    unsafe {
        gridwell_free_text_result(result);
        gridwell_free_table(table);
    };
}

#[test]
fn render_all_text_formats() {
    let json = load_fixture_json("minimal/minimal_1x1.json");
    let mut err: *mut GridwellError = ptr::null_mut();

    let table = unsafe {
        gridwell_parse_ir(json.as_ptr() as *const i8, json.len(), &mut err)
    };
    assert!(!table.is_null());

    let formats = ["html", "latex", "typst", "rtf", "svg", "ansi", "pandoc", "quarto"];

    for fmt in &formats {
        let format = CString::new(*fmt).unwrap();
        let result = unsafe { gridwell_render_text(table, format.as_ptr(), &mut err) };

        assert!(
            !result.text.is_null(),
            "render to {fmt} should produce output"
        );
        assert!(result.len > 0, "{fmt} output should be non-empty");
        assert!(err.is_null(), "{fmt} should not produce an error");

        unsafe { gridwell_free_text_result(result) };
    }

    unsafe { gridwell_free_table(table) };
}

#[test]
fn render_unknown_format() {
    let json = load_fixture_json("minimal/minimal_1x1.json");
    let mut err: *mut GridwellError = ptr::null_mut();

    let table = unsafe {
        gridwell_parse_ir(json.as_ptr() as *const i8, json.len(), &mut err)
    };
    assert!(!table.is_null());

    let format = CString::new("nosuchformat").unwrap();
    let result = unsafe { gridwell_render_text(table, format.as_ptr(), &mut err) };

    assert!(result.text.is_null(), "unknown format should fail");
    assert!(!err.is_null());

    let code = unsafe { gridwell_error_code(err) };
    assert_eq!(code, 3); // ERR_RENDER

    unsafe {
        gridwell_free_error(err);
        gridwell_free_table(table);
    };
}

// ─── Binary render tests ───

#[test]
fn render_all_binary_formats() {
    let json = load_fixture_json("minimal/minimal_1x1.json");
    let mut err: *mut GridwellError = ptr::null_mut();

    let table = unsafe {
        gridwell_parse_ir(json.as_ptr() as *const i8, json.len(), &mut err)
    };
    assert!(!table.is_null());

    let formats = ["docx", "xlsx", "pptx"];

    for fmt in &formats {
        let format = CString::new(*fmt).unwrap();
        let result = unsafe { gridwell_render_binary(table, format.as_ptr(), &mut err) };

        assert!(
            !result.data.is_null(),
            "render to {fmt} should produce output"
        );
        assert!(result.len > 100, "{fmt} output should be substantial");
        assert!(err.is_null(), "{fmt} should not produce an error");

        // Verify ZIP magic bytes
        let bytes = unsafe { std::slice::from_raw_parts(result.data, result.len) };
        assert_eq!(&bytes[0..4], b"PK\x03\x04", "{fmt} should be a valid ZIP");

        unsafe { gridwell_free_binary_result(result) };
    }

    unsafe { gridwell_free_table(table) };
}

// ─── Free null pointers (should not crash) ───

#[test]
fn free_null_pointers_safe() {
    unsafe {
        gridwell_free_table(ptr::null_mut());
        gridwell_free_error(ptr::null_mut());
        gridwell_free_text_result(GridwellTextResult {
            text: ptr::null_mut(),
            len: 0,
        });
        gridwell_free_binary_result(GridwellBinaryResult {
            data: ptr::null_mut(),
            len: 0,
        });
    }
}
