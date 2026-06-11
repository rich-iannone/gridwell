//! C FFI surface for gridwell.
//!
//! This crate exposes the gridwell table rendering pipeline through a C-compatible ABI.
//! It provides functions to parse IR JSON, validate tables, and render to all supported
//! output formats.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::ptr;
use std::slice;

use gridwell_ir::Table;

// ─── Opaque types ───

/// Opaque handle to a parsed table.
pub struct GridwellTable {
    inner: Table,
}

/// Opaque error type.
pub struct GridwellError {
    message: String,
    code: i32,
}

/// Result for text-format rendering.
#[repr(C)]
pub struct GridwellTextResult {
    pub text: *mut c_char,
    pub len: usize,
}

/// Result for binary-format rendering.
#[repr(C)]
pub struct GridwellBinaryResult {
    pub data: *mut u8,
    pub len: usize,
}

// ─── Error codes ───

const ERR_PARSE: i32 = 1;
const ERR_VALIDATE: i32 = 2;
const ERR_RENDER: i32 = 3;
const ERR_INVALID_ARG: i32 = 4;

// ─── Parsing ───

/// Parse an IR JSON string into a GridwellTable.
///
/// On success, returns a non-null pointer. On failure, returns null and
/// sets `*err` to a newly allocated error (if `err` is non-null).
///
/// # Safety
/// - `json` must be a valid pointer to `len` bytes of UTF-8 data.
/// - `err` may be null; if non-null, `*err` is written on failure.
#[no_mangle]
pub unsafe extern "C" fn gridwell_parse_ir(
    json: *const c_char,
    len: usize,
    err: *mut *mut GridwellError,
) -> *mut GridwellTable {
    if json.is_null() {
        set_error(err, ERR_INVALID_ARG, "json pointer is null");
        return ptr::null_mut();
    }

    let json_bytes = unsafe { slice::from_raw_parts(json as *const u8, len) };
    let json_str = match std::str::from_utf8(json_bytes) {
        Ok(s) => s,
        Err(e) => {
            set_error(err, ERR_PARSE, &format!("invalid UTF-8: {e}"));
            return ptr::null_mut();
        }
    };

    match Table::from_json(json_str) {
        Ok(table) => Box::into_raw(Box::new(GridwellTable { inner: table })),
        Err(e) => {
            set_error(err, ERR_PARSE, &e.to_string());
            ptr::null_mut()
        }
    }
}

// ─── Validation ───

/// Validate a parsed table. Returns null on success, or an error on failure.
///
/// # Safety
/// - `table` must be a valid pointer returned by `gridwell_parse_ir`.
#[no_mangle]
pub unsafe extern "C" fn gridwell_validate(
    table: *const GridwellTable,
) -> *mut GridwellError {
    if table.is_null() {
        return make_error(ERR_INVALID_ARG, "table pointer is null");
    }

    let table = unsafe { &(*table).inner };
    let errors = table.validate();

    if errors.is_empty() {
        ptr::null_mut()
    } else {
        let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        let combined = messages.join("; ");
        make_error(ERR_VALIDATE, &combined)
    }
}

// ─── Text rendering ───

/// Render a table to a text format.
///
/// Supported formats: "html", "latex", "typst", "rtf", "svg", "ansi", "pandoc", "quarto".
///
/// # Safety
/// - `table` must be a valid pointer returned by `gridwell_parse_ir`.
/// - `format` must be a valid null-terminated C string.
/// - `err` may be null; if non-null, `*err` is written on failure.
#[no_mangle]
pub unsafe extern "C" fn gridwell_render_text(
    table: *const GridwellTable,
    format: *const c_char,
    err: *mut *mut GridwellError,
) -> GridwellTextResult {
    let empty = GridwellTextResult {
        text: ptr::null_mut(),
        len: 0,
    };

    if table.is_null() {
        set_error(err, ERR_INVALID_ARG, "table pointer is null");
        return empty;
    }
    if format.is_null() {
        set_error(err, ERR_INVALID_ARG, "format pointer is null");
        return empty;
    }

    let table = unsafe { &(*table).inner };
    let format_str = match unsafe { CStr::from_ptr(format) }.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_error(err, ERR_INVALID_ARG, &format!("invalid format string: {e}"));
            return empty;
        }
    };

    let result = render_text_format(table, format_str);
    match result {
        Ok(text) => {
            let mut bytes = text.into_bytes();
            let len = bytes.len(); // text length without null
            bytes.push(0); // null terminator
            bytes.shrink_to_fit();
            let ptr = bytes.as_mut_ptr() as *mut c_char;
            std::mem::forget(bytes);
            GridwellTextResult { text: ptr, len }
        }
        Err(msg) => {
            set_error(err, ERR_RENDER, &msg);
            empty
        }
    }
}

// ─── Binary rendering ───

/// Render a table to a binary format.
///
/// Supported formats: "docx", "xlsx", "pptx".
///
/// # Safety
/// - `table` must be a valid pointer returned by `gridwell_parse_ir`.
/// - `format` must be a valid null-terminated C string.
/// - `err` may be null; if non-null, `*err` is written on failure.
#[no_mangle]
pub unsafe extern "C" fn gridwell_render_binary(
    table: *const GridwellTable,
    format: *const c_char,
    err: *mut *mut GridwellError,
) -> GridwellBinaryResult {
    let empty = GridwellBinaryResult {
        data: ptr::null_mut(),
        len: 0,
    };

    if table.is_null() {
        set_error(err, ERR_INVALID_ARG, "table pointer is null");
        return empty;
    }
    if format.is_null() {
        set_error(err, ERR_INVALID_ARG, "format pointer is null");
        return empty;
    }

    let table = unsafe { &(*table).inner };
    let format_str = match unsafe { CStr::from_ptr(format) }.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_error(err, ERR_INVALID_ARG, &format!("invalid format string: {e}"));
            return empty;
        }
    };

    let result = render_binary_format(table, format_str);
    match result {
        Ok(bytes) => {
            let len = bytes.len();
            let ptr = vec_to_raw_ptr(bytes);
            GridwellBinaryResult { data: ptr, len }
        }
        Err(msg) => {
            set_error(err, ERR_RENDER, &msg);
            empty
        }
    }
}

// ─── Memory management ───

/// Free a table returned by `gridwell_parse_ir`.
///
/// # Safety
/// - `table` must be a pointer returned by `gridwell_parse_ir`, or null.
#[no_mangle]
pub unsafe extern "C" fn gridwell_free_table(table: *mut GridwellTable) {
    if !table.is_null() {
        drop(unsafe { Box::from_raw(table) });
    }
}

/// Free a text result returned by `gridwell_render_text`.
///
/// # Safety
/// - `result.text` must have been allocated by this library, or be null.
#[no_mangle]
pub unsafe extern "C" fn gridwell_free_text_result(result: GridwellTextResult) {
    if !result.text.is_null() {
        // The allocation is len + 1 (includes null terminator)
        let alloc_len = result.len + 1;
        drop(unsafe {
            Vec::from_raw_parts(result.text as *mut u8, alloc_len, alloc_len)
        });
    }
}

/// Free a binary result returned by `gridwell_render_binary`.
///
/// # Safety
/// - `result.data` must have been allocated by this library, or be null.
#[no_mangle]
pub unsafe extern "C" fn gridwell_free_binary_result(result: GridwellBinaryResult) {
    if !result.data.is_null() {
        drop(unsafe { Vec::from_raw_parts(result.data, result.len, result.len) });
    }
}

/// Free an error returned by any gridwell function.
///
/// # Safety
/// - `err` must be a pointer returned by a gridwell function, or null.
#[no_mangle]
pub unsafe extern "C" fn gridwell_free_error(err: *mut GridwellError) {
    if !err.is_null() {
        drop(unsafe { Box::from_raw(err) });
    }
}

// ─── Error inspection ───

/// Get the error message as a null-terminated C string.
/// The returned pointer is valid until `gridwell_free_error` is called.
///
/// # Safety
/// - `err` must be a valid pointer to a GridwellError.
#[no_mangle]
pub unsafe extern "C" fn gridwell_error_message(
    err: *const GridwellError,
) -> *const c_char {
    if err.is_null() {
        return b"(null error)\0".as_ptr() as *const c_char;
    }
    let err = unsafe { &*err };
    // The message string has a trailing null byte appended during construction
    err.message.as_ptr() as *const c_char
}

/// Get the error code.
///
/// # Safety
/// - `err` must be a valid pointer to a GridwellError.
#[no_mangle]
pub unsafe extern "C" fn gridwell_error_code(err: *const GridwellError) -> i32 {
    if err.is_null() {
        return 0;
    }
    unsafe { &*err }.code
}

// ─── Internal helpers ───

fn render_text_format(table: &Table, format: &str) -> Result<String, String> {
    match format {
        "html" => gridwell_writer_html::HtmlWriter::new()
            .render(table)
            .map_err(|e| e.to_string()),
        "latex" => gridwell_writer_latex::LatexWriter::new()
            .render(table)
            .map_err(|e| e.to_string()),
        "typst" => gridwell_writer_typst::TypstWriter::new()
            .render(table)
            .map_err(|e| e.to_string()),
        "rtf" => gridwell_writer_rtf::RtfWriter::new()
            .render(table)
            .map_err(|e| e.to_string()),
        "svg" => gridwell_writer_svg::SvgWriter::new()
            .render(table)
            .map_err(|e| e.to_string()),
        "ansi" => gridwell_writer_ansi::AnsiWriter::new()
            .render(table)
            .map_err(|e| e.to_string()),
        "pandoc" => gridwell_writer_pandoc::PandocWriter::new()
            .render(table)
            .map_err(|e| e.to_string()),
        "quarto" => gridwell_writer_quarto::QuartoWriter::new()
            .render(table)
            .map_err(|e| e.to_string()),
        _ => Err(format!("unknown text format: \"{format}\"")),
    }
}

fn render_binary_format(table: &Table, format: &str) -> Result<Vec<u8>, String> {
    match format {
        "docx" => gridwell_writer_docx::DocxWriter::new()
            .render(table)
            .map_err(|e| e.to_string()),
        "xlsx" => gridwell_writer_xlsx::XlsxWriter::new()
            .render(table)
            .map_err(|e| e.to_string()),
        "pptx" => gridwell_writer_pptx::PptxWriter::new()
            .render(table)
            .map_err(|e| e.to_string()),
        _ => Err(format!("unknown binary format: \"{format}\"")),
    }
}

fn set_error(err: *mut *mut GridwellError, code: i32, message: &str) {
    if !err.is_null() {
        unsafe { *err = make_error(code, message) };
    }
}

fn make_error(code: i32, message: &str) -> *mut GridwellError {
    // Append null terminator so error_message can return it as a C string
    let mut msg = message.to_string();
    msg.push('\0');
    Box::into_raw(Box::new(GridwellError { message: msg, code }))
}

fn vec_to_raw_ptr(mut v: Vec<u8>) -> *mut u8 {
    v.shrink_to_fit();
    let ptr = v.as_mut_ptr();
    std::mem::forget(v);
    ptr
}
