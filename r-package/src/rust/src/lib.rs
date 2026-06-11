use extendr_api::prelude::*;
use gridwell_ir::Table;

// ─── Helpers ───

macro_rules! with_table {
    ($table_ptr:expr, |$table:ident| $body:expr) => {{
        let ptr: ExternalPtr<Table> = (&$table_ptr).try_into().unwrap_or_else(|_| {
            panic!(
                "Expected a gridwell table pointer. Did you pass the result of gw_parse_ir()?"
            )
        });
        let $table = ptr.as_ref();
        $body
    }};
}

fn throw_r_error(msg: &str) -> ! {
    panic!("{msg}");
}

// ─── Parse ───

/// Parse a gridwell table from a JSON string.
/// @param json A JSON string containing the table IR.
/// @return An external pointer to the parsed table.
/// @export
#[extendr]
fn gw_parse_ir(json: &str) -> Robj {
    match Table::from_json(json) {
        Ok(table) => ExternalPtr::new(table).into(),
        Err(e) => {
            throw_r_error(&format!("Failed to parse IR: {e}"));
        }
    }
}

// ─── Validate ───

/// Validate a parsed table, returning a character vector of errors.
/// @param table_ptr An external pointer to a parsed table.
/// @return A character vector of validation errors (empty if valid).
/// @export
#[extendr]
fn gw_validate(table_ptr: Robj) -> Strings {
    with_table!(table_ptr, |table| {
        let errors: Vec<String> =
            table.validate().into_iter().map(|e| e.to_string()).collect();
        Strings::from_values(errors)
    })
}

// ─── Serialize ───

/// Serialize a parsed table back to a JSON string.
/// @param table_ptr An external pointer to a parsed table.
/// @return A JSON string.
/// @export
#[extendr]
fn gw_to_json(table_ptr: Robj) -> String {
    with_table!(table_ptr, |table| {
        match table.to_json() {
            Ok(json) => json,
            Err(e) => throw_r_error(&format!("Failed to serialize: {e}")),
        }
    })
}

// ─── Text renderers ───

/// Render a table to HTML.
/// @param table_ptr An external pointer to a parsed table.
/// @return An HTML string.
/// @export
#[extendr]
fn gw_render_html(table_ptr: Robj) -> String {
    with_table!(table_ptr, |table| {
        match gridwell_writer_html::render_html(table) {
            Ok(s) => s,
            Err(e) => throw_r_error(&format!("HTML render error: {e}")),
        }
    })
}

/// Render a table to LaTeX.
/// @param table_ptr An external pointer to a parsed table.
/// @return A LaTeX string.
/// @export
#[extendr]
fn gw_render_latex(table_ptr: Robj) -> String {
    with_table!(table_ptr, |table| {
        match gridwell_writer_latex::render_latex(table) {
            Ok(s) => s,
            Err(e) => throw_r_error(&format!("LaTeX render error: {e}")),
        }
    })
}

/// Render a table to Typst.
/// @param table_ptr An external pointer to a parsed table.
/// @return A Typst string.
/// @export
#[extendr]
fn gw_render_typst(table_ptr: Robj) -> String {
    with_table!(table_ptr, |table| {
        match gridwell_writer_typst::render_typst(table) {
            Ok(s) => s,
            Err(e) => throw_r_error(&format!("Typst render error: {e}")),
        }
    })
}

/// Render a table to RTF.
/// @param table_ptr An external pointer to a parsed table.
/// @return An RTF string.
/// @export
#[extendr]
fn gw_render_rtf(table_ptr: Robj) -> String {
    with_table!(table_ptr, |table| {
        match gridwell_writer_rtf::render_rtf(table) {
            Ok(s) => s,
            Err(e) => throw_r_error(&format!("RTF render error: {e}")),
        }
    })
}

/// Render a table to SVG.
/// @param table_ptr An external pointer to a parsed table.
/// @return An SVG string.
/// @export
#[extendr]
fn gw_render_svg(table_ptr: Robj) -> String {
    with_table!(table_ptr, |table| {
        match gridwell_writer_svg::render_svg(table) {
            Ok(s) => s,
            Err(e) => throw_r_error(&format!("SVG render error: {e}")),
        }
    })
}

/// Render a table with ANSI escape codes.
/// @param table_ptr An external pointer to a parsed table.
/// @return A string with ANSI codes.
/// @export
#[extendr]
fn gw_render_ansi(table_ptr: Robj) -> String {
    with_table!(table_ptr, |table| {
        match gridwell_writer_ansi::render_ansi(table) {
            Ok(s) => s,
            Err(e) => throw_r_error(&format!("ANSI render error: {e}")),
        }
    })
}

/// Render a table to Pandoc AST JSON.
/// @param table_ptr An external pointer to a parsed table.
/// @return A Pandoc JSON string.
/// @export
#[extendr]
fn gw_render_pandoc(table_ptr: Robj) -> String {
    with_table!(table_ptr, |table| {
        match gridwell_writer_pandoc::render_pandoc(table) {
            Ok(s) => s,
            Err(e) => throw_r_error(&format!("Pandoc render error: {e}")),
        }
    })
}

/// Render a table to Quarto-flavored Markdown.
/// @param table_ptr An external pointer to a parsed table.
/// @return A Quarto Markdown string.
/// @export
#[extendr]
fn gw_render_quarto(table_ptr: Robj) -> String {
    with_table!(table_ptr, |table| {
        match gridwell_writer_quarto::render_quarto(table) {
            Ok(s) => s,
            Err(e) => throw_r_error(&format!("Quarto render error: {e}")),
        }
    })
}

/// Render a table to a named text format.
/// @param table_ptr An external pointer to a parsed table.
/// @param format One of: "html", "latex", "typst", "rtf", "svg", "ansi", "pandoc", "quarto".
/// @return The rendered string.
/// @export
#[extendr]
fn gw_render(table_ptr: Robj, format: &str) -> String {
    with_table!(table_ptr, |table| {
        let result = match format {
            "html" => gridwell_writer_html::render_html(table).map_err(|e| e.to_string()),
            "latex" => gridwell_writer_latex::render_latex(table).map_err(|e| e.to_string()),
            "typst" => gridwell_writer_typst::render_typst(table).map_err(|e| e.to_string()),
            "rtf" => gridwell_writer_rtf::render_rtf(table).map_err(|e| e.to_string()),
            "svg" => gridwell_writer_svg::render_svg(table).map_err(|e| e.to_string()),
            "ansi" => gridwell_writer_ansi::render_ansi(table).map_err(|e| e.to_string()),
            "pandoc" => {
                gridwell_writer_pandoc::render_pandoc(table).map_err(|e| e.to_string())
            }
            "quarto" => {
                gridwell_writer_quarto::render_quarto(table).map_err(|e| e.to_string())
            }
            _ => Err(format!(
                "Unknown format: '{format}'. \
                 Supported: html, latex, typst, rtf, svg, ansi, pandoc, quarto"
            )),
        };
        match result {
            Ok(s) => s,
            Err(e) => throw_r_error(&e),
        }
    })
}

// ─── Binary renderers ───

/// Render a table to DOCX (returns a raw vector).
/// @param table_ptr An external pointer to a parsed table.
/// @return A raw vector containing the DOCX file bytes.
/// @export
#[extendr]
fn gw_render_docx(table_ptr: Robj) -> Raw {
    with_table!(table_ptr, |table| {
        match gridwell_writer_docx::render_docx(table) {
            Ok(bytes) => Raw::from_bytes(&bytes),
            Err(e) => throw_r_error(&format!("DOCX render error: {e}")),
        }
    })
}

/// Render a table to XLSX (returns a raw vector).
/// @param table_ptr An external pointer to a parsed table.
/// @return A raw vector containing the XLSX file bytes.
/// @export
#[extendr]
fn gw_render_xlsx(table_ptr: Robj) -> Raw {
    with_table!(table_ptr, |table| {
        match gridwell_writer_xlsx::render_xlsx(table) {
            Ok(bytes) => Raw::from_bytes(&bytes),
            Err(e) => throw_r_error(&format!("XLSX render error: {e}")),
        }
    })
}

/// Render a table to PPTX (returns a raw vector).
/// @param table_ptr An external pointer to a parsed table.
/// @return A raw vector containing the PPTX file bytes.
/// @export
#[extendr]
fn gw_render_pptx(table_ptr: Robj) -> Raw {
    with_table!(table_ptr, |table| {
        match gridwell_writer_pptx::render_pptx(table) {
            Ok(bytes) => Raw::from_bytes(&bytes),
            Err(e) => throw_r_error(&format!("PPTX render error: {e}")),
        }
    })
}

/// Render a table to a named binary format (returns a raw vector).
/// @param table_ptr An external pointer to a parsed table.
/// @param format One of: "docx", "xlsx", "pptx".
/// @return A raw vector containing the file bytes.
/// @export
#[extendr]
fn gw_render_binary(table_ptr: Robj, format: &str) -> Raw {
    with_table!(table_ptr, |table| {
        let result = match format {
            "docx" => gridwell_writer_docx::render_docx(table).map_err(|e| e.to_string()),
            "xlsx" => gridwell_writer_xlsx::render_xlsx(table).map_err(|e| e.to_string()),
            "pptx" => gridwell_writer_pptx::render_pptx(table).map_err(|e| e.to_string()),
            _ => Err(format!(
                "Unknown binary format: '{format}'. Supported: docx, xlsx, pptx"
            )),
        };
        match result {
            Ok(bytes) => Raw::from_bytes(&bytes),
            Err(e) => throw_r_error(&e),
        }
    })
}

extendr_module! {
    mod gridwell;
    fn gw_parse_ir;
    fn gw_validate;
    fn gw_to_json;
    fn gw_render_html;
    fn gw_render_latex;
    fn gw_render_typst;
    fn gw_render_rtf;
    fn gw_render_svg;
    fn gw_render_ansi;
    fn gw_render_pandoc;
    fn gw_render_quarto;
    fn gw_render;
    fn gw_render_docx;
    fn gw_render_xlsx;
    fn gw_render_pptx;
    fn gw_render_binary;
}
