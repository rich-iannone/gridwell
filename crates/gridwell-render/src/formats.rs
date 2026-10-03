//! The registered formats. Adding a format means adding an entry to [`REGISTRY`]
//! and nowhere else.

use gridwell_ir::ValidatedTable;
use serde::Serialize;
use serde_json::Value;

use crate::{parse_options, Output, OutputKind, RenderError, Writer};

/// Every format gridwell can write, text formats first.
pub static REGISTRY: &[&dyn Writer] = &[
    &Html, &Latex, &Typst, &Rtf, &Svg, &Ansi, &Pandoc, &Quarto, &Docx, &Xlsx, &Pptx,
];

/// A writer error, labelled with the format.
fn failed(format: &'static str, e: impl std::fmt::Display) -> RenderError {
    RenderError::Writer {
        format,
        message: e.to_string(),
    }
}

fn invalid(format: &str, message: impl Into<String>) -> RenderError {
    RenderError::InvalidOptions {
        format: format.to_string(),
        message: message.into(),
    }
}

fn defaults<T: Serialize + Default>() -> Value {
    serde_json::to_value(T::default()).expect("option structs serialize")
}

/// Declare a format. `$options` is the writer's config type (or `()` for none);
/// `$check` validates parsed options; `$render` turns (table, options) into the
/// writer's result.
macro_rules! define_format {
    (
        $ty:ident, $name:literal, $desc:literal, $kind:ident, $ext:literal, $media:literal,
        options: $options:ty,
        check: $check:expr,
        render: $render:expr $(,)?
    ) => {
        pub struct $ty;

        impl Writer for $ty {
            fn name(&self) -> &'static str {
                $name
            }
            fn description(&self) -> &'static str {
                $desc
            }
            fn kind(&self) -> OutputKind {
                OutputKind::$kind
            }
            fn extension(&self) -> &'static str {
                $ext
            }
            fn media_type(&self) -> &'static str {
                $media
            }
            fn default_options(&self) -> Value {
                defaults::<$options>()
            }
            fn render(
                &self,
                table: &ValidatedTable<'_>,
                options: Option<&Value>,
            ) -> Result<Output, RenderError> {
                let opts: $options = parse_options($name, options)?;
                let check: fn(&$options) -> Result<(), String> = $check;
                check(&opts).map_err(|m| invalid($name, m))?;
                let render: fn(&gridwell_ir::Table, $options) -> Result<_, _> = $render;
                render(table.table(), opts)
                    .map(Into::into)
                    .map_err(|e| failed($name, e))
            }
        }
    };
}

impl From<String> for Output {
    fn from(s: String) -> Self {
        Output::Text(s)
    }
}

impl From<Vec<u8>> for Output {
    fn from(b: Vec<u8>) -> Self {
        Output::Binary(b)
    }
}

/// No options: only `{}` / `null` are accepted.
#[derive(Debug, Default, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoOptions {}

fn ok<T>(_: &T) -> Result<(), String> {
    Ok(())
}

define_format!(
    Html, "html", "HTML5 table", Text, "html", "text/html",
    options: gridwell_writer_html::HtmlWriterConfig,
    check: |o| css_identifier("class_prefix", &o.class_prefix),
    render: |t, o| gridwell_writer_html::HtmlWriter::with_config(o).render(t),
);
define_format!(
    Latex, "latex", "LaTeX tabular / longtable (booktabs)", Text, "tex", "application/x-latex",
    options: gridwell_writer_latex::LatexWriterConfig,
    check: ok,
    render: |t, o| gridwell_writer_latex::LatexWriter::with_config(o).render(t),
);
define_format!(
    Typst, "typst", "Typst table markup", Text, "typ", "text/x-typst",
    options: gridwell_writer_typst::TypstWriterConfig,
    check: ok,
    render: |t, o| gridwell_writer_typst::TypstWriter::with_config(o).render(t),
);
define_format!(
    Rtf, "rtf", "Rich Text Format", Text, "rtf", "application/rtf",
    options: NoOptions,
    check: ok,
    render: |t, _| gridwell_writer_rtf::render_rtf(t),
);
define_format!(
    Svg, "svg", "SVG image (standalone, measured layout)", Text, "svg", "image/svg+xml",
    options: gridwell_writer_svg::SvgConfig,
    check: check_svg,
    render: |t, o| gridwell_writer_svg::SvgWriter::with_config(o).render(t),
);
define_format!(
    Ansi, "ansi", "Terminal output with box drawing and ANSI colours", Text, "txt", "text/plain",
    options: gridwell_writer_ansi::AnsiConfig,
    check: ok,
    render: |t, o| gridwell_writer_ansi::AnsiWriter::with_config(o).render(t),
);
define_format!(
    Pandoc, "pandoc", "Pandoc JSON AST (Table block)", Text, "json", "application/json",
    options: NoOptions,
    check: ok,
    render: |t, _| gridwell_writer_pandoc::render_pandoc(t),
);
define_format!(
    Quarto, "quarto", "Quarto-flavoured Pandoc JSON AST (cross-referenceable Div)", Text, "json",
    "application/json",
    options: gridwell_writer_quarto::QuartoConfig,
    check: ok,
    render: |t, o| gridwell_writer_quarto::QuartoWriter::with_config(o).render(t),
);
define_format!(
    Docx, "docx", "Microsoft Word document", Binary, "docx",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    options: NoOptions,
    check: ok,
    render: |t, _| gridwell_writer_docx::render_docx(t),
);
define_format!(
    Xlsx, "xlsx", "Microsoft Excel workbook", Binary, "xlsx",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    options: NoOptions,
    check: ok,
    render: |t, _| gridwell_writer_xlsx::render_xlsx(t),
);
define_format!(
    Pptx, "pptx", "Microsoft PowerPoint slide", Binary, "pptx",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    options: NoOptions,
    check: ok,
    render: |t, _| gridwell_writer_pptx::render_pptx(t),
);

/// A CSS class-name prefix: it is written into `class` attributes and selectors,
/// so it must be a plain identifier (`[A-Za-z_][A-Za-z0-9_-]*`).
fn css_identifier(field: &str, v: &str) -> Result<(), String> {
    let mut chars = v.chars();
    let ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "{field} must start with a letter or `_` and contain only letters, digits, `_` and `-`; got {v:?}"
        ))
    }
}

/// SVG sizes must be finite and positive (padding may be zero), and the font
/// family is written into a `<style>` block, so it is limited to the characters a
/// font list needs.
fn check_svg(o: &gridwell_writer_svg::SvgConfig) -> Result<(), String> {
    for (field, v, zero_ok) in [
        ("font_size", o.font_size, false),
        ("row_height", o.row_height, false),
        ("default_col_width", o.default_col_width, false),
        ("cell_padding_x", o.cell_padding_x, true),
        ("cell_padding_y", o.cell_padding_y, true),
    ] {
        let ok = v.is_finite() && (v > 0.0 || (zero_ok && v == 0.0)) && v <= 10_000.0;
        if !ok {
            let range = if zero_ok {
                "0–10000"
            } else {
                "greater than 0, at most 10000"
            };
            return Err(format!("{field} must be {range}; got {v}"));
        }
    }
    let family_ok = !o.font_family.trim().is_empty()
        && o.font_family
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | ',' | '-' | '_' | '.' | '"' | '\''));
    if !family_ok {
        return Err(format!(
            "font_family may contain only letters, digits, spaces, quotes and , - _ .; got {:?}",
            o.font_family
        ));
    }
    Ok(())
}
