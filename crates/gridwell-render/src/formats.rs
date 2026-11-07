//! The registered formats. Adding a format means adding an entry to [`REGISTRY`]
//! and a cargo feature named as the format is, here and in the `gridwell` crate.

// With some formats off, what only the others use goes unused. Every format is
// on by default, and that build is linted in full.
#![cfg_attr(
    not(all(
        feature = "html",
        feature = "latex",
        feature = "typst",
        feature = "rtf",
        feature = "svg",
        feature = "ansi",
        feature = "pandoc",
        feature = "quarto",
        feature = "docx",
        feature = "xlsx",
        feature = "pptx"
    )),
    allow(dead_code, unused_imports, unused_macros)
)]

use gridwell_ir::ValidatedTable;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::{Output, OutputKind, RenderError, Unsupported, Writer};

/// Every format gridwell can write, text formats first: those whose cargo
/// features are on (every one, by default).
pub static REGISTRY: &[&dyn Writer] = &[
    #[cfg(feature = "html")]
    &Html,
    #[cfg(feature = "latex")]
    &Latex,
    #[cfg(feature = "typst")]
    &Typst,
    #[cfg(feature = "rtf")]
    &Rtf,
    #[cfg(feature = "svg")]
    &Svg,
    #[cfg(feature = "ansi")]
    &Ansi,
    #[cfg(feature = "pandoc")]
    &Pandoc,
    #[cfg(feature = "quarto")]
    &Quarto,
    #[cfg(feature = "docx")]
    &Docx,
    #[cfg(feature = "xlsx")]
    &Xlsx,
    #[cfg(feature = "pptx")]
    &Pptx,
];

/// Deserialize a format's options: `None`/`null` gives the defaults; anything else
/// must be a JSON object whose fields the format knows.
fn parse_options<T: DeserializeOwned + Default>(
    format: &str,
    options: Option<&Value>,
) -> Result<T, RenderError> {
    let invalid = |message: String| RenderError::InvalidOptions {
        format: format.to_string(),
        message,
    };
    match options {
        None | Some(Value::Null) => Ok(T::default()),
        Some(v @ Value::Object(_)) => T::deserialize(v).map_err(|e| invalid(e.to_string())),
        Some(other) => Err(invalid(format!("expected a JSON object, got {other}"))),
    }
}

/// A writer error, labeled with the format.
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

/// Declare a format. `$experimental` is [`Writer::experimental`]; `$options` is
/// the writer's config type (or `()` for none); `$check` validates parsed
/// options; `$render` turns (table, options) into the writer's result;
/// `$unsupported`, if given, lists what the format can't express of a table.
macro_rules! define_format {
    (
        $ty:ident, $name:literal, $desc:literal, $kind:ident, $ext:literal, $media:literal,
        experimental: $experimental:literal,
        options: $options:ty,
        check: $check:expr,
        render: $render:expr
        $(, unsupported: $unsupported:expr)? $(,)?
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
            fn experimental(&self) -> bool {
                $experimental
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
            $(
                fn unsupported(&self, table: &ValidatedTable<'_>) -> Vec<Unsupported> {
                    let unsupported: fn(&gridwell_ir::Table) -> Vec<Unsupported> = $unsupported;
                    unsupported(table.table())
                }
            )?
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

#[cfg(feature = "html")]
define_format!(
    Html, "html", "HTML5 table", Text, "html", "text/html",
    experimental: false,
    options: gridwell_writer_html::HtmlWriterConfig,
    check: gridwell_writer_html::check_config,
    render: |t, o| gridwell_writer_html::HtmlWriter::with_config(o).render(t),
);
#[cfg(feature = "latex")]
define_format!(
    Latex, "latex", "LaTeX tabular / longtable (booktabs)", Text, "tex", "application/x-latex",
    experimental: false,
    options: gridwell_writer_latex::LatexWriterConfig,
    check: ok,
    render: |t, o| gridwell_writer_latex::LatexWriter::with_config(o).render(t),
    unsupported: gridwell_writer_latex::unsupported,
);
#[cfg(feature = "typst")]
define_format!(
    Typst, "typst", "Typst table markup", Text, "typ", "text/x-typst",
    experimental: false,
    options: gridwell_writer_typst::TypstWriterConfig,
    check: ok,
    render: |t, o| gridwell_writer_typst::TypstWriter::with_config(o).render(t),
    unsupported: gridwell_writer_typst::unsupported,
);
#[cfg(feature = "rtf")]
define_format!(
    Rtf, "rtf", "Rich Text Format", Text, "rtf", "application/rtf",
    experimental: false,
    options: NoOptions,
    check: ok,
    render: |t, _| gridwell_writer_rtf::render_rtf(t),
    unsupported: gridwell_writer_rtf::unsupported,
);
#[cfg(feature = "svg")]
define_format!(
    Svg, "svg", "SVG image (standalone, measured layout)", Text, "svg", "image/svg+xml",
    experimental: true,
    options: gridwell_writer_svg::SvgConfig,
    check: check_svg,
    render: |t, o| gridwell_writer_svg::SvgWriter::with_config(o).render(t),
    unsupported: gridwell_writer_svg::unsupported,
);
#[cfg(feature = "ansi")]
define_format!(
    Ansi, "ansi", "Terminal output with box drawing and ANSI colors", Text, "txt", "text/plain",
    experimental: true,
    options: gridwell_writer_ansi::AnsiConfig,
    check: ok,
    render: |t, o| gridwell_writer_ansi::AnsiWriter::with_config(o).render(t),
    unsupported: gridwell_writer_ansi::unsupported,
);
#[cfg(feature = "pandoc")]
define_format!(
    Pandoc, "pandoc", "Pandoc JSON AST (Table block)", Text, "json", "application/json",
    experimental: true,
    options: NoOptions,
    check: ok,
    render: |t, _| gridwell_writer_pandoc::render_pandoc(t),
);
#[cfg(feature = "quarto")]
define_format!(
    Quarto, "quarto", "Quarto-flavored Pandoc JSON AST (cross-referenceable Div)", Text, "json",
    "application/json",
    experimental: true,
    options: gridwell_writer_quarto::QuartoConfig,
    check: ok,
    render: |t, o| gridwell_writer_quarto::QuartoWriter::with_config(o).render(t),
);
#[cfg(feature = "docx")]
define_format!(
    Docx, "docx", "Microsoft Word document", Binary, "docx",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    experimental: false,
    options: NoOptions,
    check: ok,
    render: |t, _| gridwell_writer_docx::render_docx(t),
    unsupported: gridwell_writer_docx::unsupported,
);
#[cfg(feature = "xlsx")]
define_format!(
    Xlsx, "xlsx", "Microsoft Excel workbook", Binary, "xlsx",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    experimental: false,
    options: NoOptions,
    check: ok,
    render: |t, _| gridwell_writer_xlsx::render_xlsx(t),
    unsupported: gridwell_writer_xlsx::unsupported,
);
#[cfg(feature = "pptx")]
define_format!(
    Pptx, "pptx", "Microsoft PowerPoint slide", Binary, "pptx",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    experimental: true,
    options: NoOptions,
    check: ok,
    render: |t, _| gridwell_writer_pptx::render_pptx(t),
    unsupported: gridwell_writer_pptx::unsupported,
);

/// SVG sizes must be finite and positive (padding may be zero), and the font
/// family is written into a `<style>` block, so it is limited to the characters a
/// font list needs.
#[cfg(feature = "svg")]
fn check_svg(o: &gridwell_writer_svg::SvgConfig) -> Result<(), String> {
    for (field, v, zero_ok) in [
        ("font_size", o.font_size, false),
        ("row_height", o.row_height, true),
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
