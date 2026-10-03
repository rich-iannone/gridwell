//! The format registry: every output format behind one [`Writer`] trait, looked up
//! by name, configured with JSON options, and rendered from validated IR.
//!
//! This is the only place that knows the list of formats. The CLI, the C FFI and
//! the Python and R bindings all render through [`render`] (or [`find`]), so they
//! accept the same format names and options and report the same errors.
//!
//! ```
//! # let json = r#"{"ir_version":"1.0","config":{"table_cols":1,"body_rows":1},
//! #   "styles":{"defs":{},"compositions":{},"conditionals":[]},
//! #   "column_spec":[{"id":"a"}],
//! #   "table":{"thead":{"rows":[]},"tbody":[{"rows":[{"cells":[{"content":[]}]}]}]}}"#;
//! let table = gridwell_ir::Table::from_json(json).unwrap();
//! let options = serde_json::json!({ "inline_styles": true });
//! let html = gridwell_render::render(&table, "html", Some(&options)).unwrap();
//! assert!(html.as_text().unwrap().contains("<table"));
//! ```

mod formats;

use gridwell_ir::{InvalidTable, Table, ValidatedTable};
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;

pub use formats::REGISTRY;

/// Whether a format produces text or bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    /// UTF-8 text.
    Text,
    /// A binary file (an OOXML zip package).
    Binary,
}

impl std::fmt::Display for OutputKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            OutputKind::Text => "text",
            OutputKind::Binary => "binary",
        })
    }
}

/// A rendered table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    Text(String),
    Binary(Vec<u8>),
}

impl Output {
    pub fn kind(&self) -> OutputKind {
        match self {
            Output::Text(_) => OutputKind::Text,
            Output::Binary(_) => OutputKind::Binary,
        }
    }

    /// The text, for text formats.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Output::Text(s) => Some(s),
            Output::Binary(_) => None,
        }
    }

    /// The bytes to write to a file (UTF-8 for text formats).
    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            Output::Text(s) => s.into_bytes(),
            Output::Binary(b) => b,
        }
    }
}

/// Everything that can go wrong between a format name and its output.
#[derive(Debug, Error)]
pub enum RenderError {
    /// No format has this name.
    #[error("unknown format \"{name}\"; supported formats: {}", names().join(", "))]
    UnknownFormat { name: String },
    /// A text-only or binary-only API was asked for a format of the other kind.
    #[error("\"{format}\" is a {actual} format, not {expected}")]
    WrongKind {
        format: &'static str,
        expected: OutputKind,
        actual: OutputKind,
    },
    /// The IR failed validation.
    #[error(transparent)]
    InvalidTable(#[from] InvalidTable),
    /// The options are not valid for the format (unknown field, wrong type, value
    /// out of range, malformed JSON).
    #[error("invalid options for \"{format}\": {message}")]
    InvalidOptions { format: String, message: String },
    /// The writer itself failed.
    #[error("{format} writer failed: {message}")]
    Writer {
        format: &'static str,
        message: String,
    },
}

/// One output format.
///
/// Implementations take a [`ValidatedTable`], so they can never be handed malformed
/// IR, and options as JSON (`None` or `null` means all defaults).
pub trait Writer: Send + Sync {
    /// The name formats are looked up by (lowercase), e.g. `"html"`.
    fn name(&self) -> &'static str;
    /// A one-line description.
    fn description(&self) -> &'static str;
    fn kind(&self) -> OutputKind;
    /// The usual file extension, without the dot.
    fn extension(&self) -> &'static str;
    /// The IANA media type (or the conventional one where none is registered).
    fn media_type(&self) -> &'static str;
    /// The options this format accepts, with their default values (an empty object
    /// for formats without options).
    fn default_options(&self) -> Value;
    /// Render the table.
    fn render(
        &self,
        table: &ValidatedTable<'_>,
        options: Option<&Value>,
    ) -> Result<Output, RenderError>;
}

/// Every format name, in registry order.
pub fn names() -> Vec<&'static str> {
    REGISTRY.iter().map(|w| w.name()).collect()
}

/// Look up a format by name (case-insensitive, surrounding whitespace ignored).
pub fn find(name: &str) -> Result<&'static dyn Writer, RenderError> {
    let key = name.trim().to_ascii_lowercase();
    REGISTRY
        .iter()
        .copied()
        .find(|w| w.name() == key)
        .ok_or_else(|| RenderError::UnknownFormat {
            name: name.to_string(),
        })
}

/// Validate `table`, then render it to `format` with `options`.
///
/// The format is looked up (and the options checked) before validation, so a typo
/// in the format name is reported even for invalid IR.
pub fn render(table: &Table, format: &str, options: Option<&Value>) -> Result<Output, RenderError> {
    let writer = find(format)?;
    let validated = table.validated()?;
    writer.render(&validated, options)
}

/// [`render`] with options given as a JSON string (as the FFI and the bindings
/// receive them). `None` or an empty string means all defaults.
pub fn render_with_json_options(
    table: &Table,
    format: &str,
    options: Option<&str>,
) -> Result<Output, RenderError> {
    let writer = find(format)?;
    let options =
        match options.map(str::trim).filter(|s| !s.is_empty()) {
            None => None,
            Some(json) => Some(serde_json::from_str::<Value>(json).map_err(|e| {
                RenderError::InvalidOptions {
                    format: writer.name().to_string(),
                    message: format!("not valid JSON: {e}"),
                }
            })?),
        };
    let validated = table.validated()?;
    writer.render(&validated, options.as_ref())
}

/// [`render`] for a text format; a binary format is a [`RenderError::WrongKind`].
pub fn render_text(
    table: &Table,
    format: &str,
    options: Option<&str>,
) -> Result<String, RenderError> {
    expect_kind(format, OutputKind::Text)?;
    match render_with_json_options(table, format, options)? {
        Output::Text(s) => Ok(s),
        Output::Binary(_) => unreachable!("kind checked"),
    }
}

/// [`render`] for a binary format; a text format is a [`RenderError::WrongKind`].
pub fn render_binary(
    table: &Table,
    format: &str,
    options: Option<&str>,
) -> Result<Vec<u8>, RenderError> {
    expect_kind(format, OutputKind::Binary)?;
    match render_with_json_options(table, format, options)? {
        Output::Binary(b) => Ok(b),
        Output::Text(_) => unreachable!("kind checked"),
    }
}

fn expect_kind(format: &str, expected: OutputKind) -> Result<(), RenderError> {
    let writer = find(format)?;
    if writer.kind() == expected {
        Ok(())
    } else {
        Err(RenderError::WrongKind {
            format: writer.name(),
            expected,
            actual: writer.kind(),
        })
    }
}

/// Deserialize a format's options: `None`/`null` gives the defaults; anything else
/// must be a JSON object whose fields the format knows.
pub(crate) fn parse_options<T: DeserializeOwned + Default>(
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
