//! The R binding: thin internal entry points over `gridwell_render`. The exported R
//! API (argument handling, options as lists or JSON) is in `R/gridwell.R`.

use extendr_api::prelude::*;
use gridwell_ir::Table;
use gridwell_render::{RenderError, REGISTRY};

fn table_from(ptr: &Robj) -> ExternalPtr<Table> {
    ptr.try_into().unwrap_or_else(|_| {
        throw_r_error("Expected a gridwell table. Did you pass the result of gw_parse_ir()?")
    })
}

/// `NULL` or a JSON string.
fn options_from(options: &Robj) -> Option<String> {
    if options.is_null() {
        return None;
    }
    match options.as_str() {
        Some(s) => Some(s.to_string()),
        None => throw_r_error("`options` must be NULL or a single JSON string"),
    }
}

/// `NULL` or a format name.
fn format_from(format: &Robj) -> Option<String> {
    if format.is_null() {
        return None;
    }
    match format.as_str() {
        Some(s) => Some(s.to_string()),
        None => throw_r_error("`format` must be NULL or a single string"),
    }
}

/// extendr turns a panic in an exported function into an R error.
fn throw_r_error(msg: &str) -> ! {
    panic!("{msg}");
}

/// Parse IR JSON into an external pointer.
/// @noRd
#[extendr]
fn rs_parse_ir(json: &str) -> Robj {
    match Table::from_json(json) {
        Ok(table) => ExternalPtr::new(table).into(),
        Err(e) => throw_r_error(&format!("Failed to parse IR: {e}")),
    }
}

/// Whether `ptr` points to a table: not after `saveRDS()` and `readRDS()`, which
/// keep an external pointer but not what it points to.
/// @noRd
#[extendr]
fn rs_is_table(ptr: Robj) -> bool {
    <&Robj as TryInto<ExternalPtr<Table>>>::try_into(&ptr).is_ok()
}

/// A one-line description of the table (size, groups, title, validity).
/// @noRd
#[extendr]
fn rs_describe(table_ptr: Robj) -> String {
    gridwell_render::describe(&table_from(&table_ptr))
}

/// The HTML an HTML document shows a table as: with a fresh id (leaving R's
/// random seed alone), and left alone by Quarto.
/// @noRd
#[extendr]
fn rs_display_html(table_ptr: Robj) -> String {
    let table = table_from(&table_ptr);
    gridwell_render::display_html(&table).unwrap_or_else(|e| throw_r_error(&e.to_string()))
}

/// Validation errors (empty if valid).
/// @noRd
#[extendr]
fn rs_validate(table_ptr: Robj) -> Strings {
    let table = table_from(&table_ptr);
    Strings::from_values(table.validate().into_iter().map(|e| e.to_string()))
}

/// What a format (or every format, for `NULL`) can't express of a valid table.
/// @noRd
#[extendr]
fn rs_warnings(table_ptr: Robj, format: Robj) -> Strings {
    let table = table_from(&table_ptr);
    let format = format_from(&format);
    let found = gridwell_render::warnings(&table, format.as_deref())
        .unwrap_or_else(|e| throw_r_error(&e.to_string()));
    Strings::from_values(found.iter().map(|w| w.to_string()))
}

/// The table as JSON.
/// @noRd
#[extendr]
fn rs_to_json(table_ptr: Robj) -> String {
    match table_from(&table_ptr).to_json() {
        Ok(json) => json,
        Err(e) => throw_r_error(&format!("Failed to serialize: {e}")),
    }
}

/// Render to a text format through the registry (validates first).
/// @noRd
#[extendr]
fn rs_render_text(table_ptr: Robj, format: &str, options: Robj) -> String {
    let table = table_from(&table_ptr);
    let options = options_from(&options);
    gridwell_render::render_text(&table, format, options.as_deref())
        .unwrap_or_else(|e| throw_r_error(&e.to_string()))
}

/// Render to a binary format through the registry (validates first).
/// @noRd
#[extendr]
fn rs_render_binary(table_ptr: Robj, format: &str, options: Robj) -> Raw {
    let table = table_from(&table_ptr);
    let options = options_from(&options);
    match gridwell_render::render_binary(&table, format, options.as_deref()) {
        Ok(bytes) => Raw::from_bytes(&bytes),
        Err(e) => throw_r_error(&e.to_string()),
    }
}

/// Render and write to `path` (format inferred from the extension when `format` is
/// `NULL`); returns the format used.
/// @noRd
#[extendr]
fn rs_save(table_ptr: Robj, path: &str, format: Robj, options: Robj) -> String {
    let table = table_from(&table_ptr);
    let format = format_from(&format);
    let options = options_from(&options);
    match gridwell_render::save(
        &table,
        std::path::Path::new(path),
        format.as_deref(),
        options.as_deref(),
    ) {
        Ok(writer) => writer.name().to_string(),
        Err(
            e @ (RenderError::NoExtension { .. }
            | RenderError::UnknownExtension { .. }
            | RenderError::AmbiguousExtension { .. }),
        ) => throw_r_error(&format!("{e} with `format =`")),
        Err(e) => throw_r_error(&e.to_string()),
    }
}

/// The registry as columns: name, description, kind, extension, media_type, and
/// options (each format's default options as a JSON string).
/// @noRd
#[extendr]
fn rs_formats() -> List {
    let col = |f: fn(&dyn gridwell_render::Writer) -> String| -> Strings {
        Strings::from_values(REGISTRY.iter().map(|w| f(*w)))
    };
    list!(
        name = col(|w| w.name().to_string()),
        description = col(|w| w.description().to_string()),
        kind = col(|w| w.kind().to_string()),
        extension = col(|w| w.extension().to_string()),
        media_type = col(|w| w.media_type().to_string()),
        options = col(|w| w.default_options().to_string())
    )
}

extendr_module! {
    mod gridwell;
    fn rs_parse_ir;
    fn rs_is_table;
    fn rs_describe;
    fn rs_display_html;
    fn rs_validate;
    fn rs_warnings;
    fn rs_to_json;
    fn rs_render_text;
    fn rs_render_binary;
    fn rs_save;
    fn rs_formats;
}
