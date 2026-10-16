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
//!
//! [`save`] renders straight to a file, inferring the format from the extension.

mod formats;

use std::path::{Path, PathBuf};

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
    /// No format was given and the path has no extension to infer one from.
    #[error("cannot infer the format of \"{}\": it has no file extension; pass a format", path.display())]
    NoExtension { path: PathBuf },
    /// No format writes files with this extension.
    #[error(
        "cannot infer the format of \"{}\": no format writes \".{extension}\" files \
         (known extensions: {}); pass a format",
        path.display(),
        extensions().join(", ")
    )]
    UnknownExtension { path: PathBuf, extension: String },
    /// More than one format writes files with this extension.
    #[error(
        "cannot infer the format of \"{}\": \".{extension}\" files can be {}; pass a format",
        path.display(),
        formats.join(" or ")
    )]
    AmbiguousExtension {
        path: PathBuf,
        extension: String,
        formats: Vec<&'static str>,
    },
    /// Writing the file failed.
    #[error("cannot write \"{}\": {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
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

/// Every file extension (without the dot), in registry order, without repeats.
pub fn extensions() -> Vec<&'static str> {
    let mut exts: Vec<&'static str> = Vec::new();
    for w in REGISTRY {
        if !exts.contains(&w.extension()) {
            exts.push(w.extension());
        }
    }
    exts
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
    let options = json_options(writer, options)?;
    let validated = table.validated()?;
    writer.render(&validated, options.as_ref())
}

/// Options given as a JSON string: `None` or an empty string means all defaults.
fn json_options(writer: &dyn Writer, options: Option<&str>) -> Result<Option<Value>, RenderError> {
    match options.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(json) => {
            serde_json::from_str::<Value>(json)
                .map(Some)
                .map_err(|e| RenderError::InvalidOptions {
                    format: writer.name().to_string(),
                    message: format!("not valid JSON: {e}"),
                })
        }
    }
}

/// The format that writes files like `path`, from its extension (case-insensitive).
///
/// Errors if the path has no extension, if no format uses it, or if several do
/// (`.json` is both `pandoc` and `quarto`).
pub fn format_for_path(path: &Path) -> Result<&'static dyn Writer, RenderError> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| !e.is_empty())
        .ok_or_else(|| RenderError::NoExtension {
            path: path.to_path_buf(),
        })?;
    let key = extension.to_ascii_lowercase();
    let matches: Vec<&'static dyn Writer> = REGISTRY
        .iter()
        .copied()
        .filter(|w| w.extension() == key)
        .collect();
    match matches.as_slice() {
        [writer] => Ok(*writer),
        [] => Err(RenderError::UnknownExtension {
            path: path.to_path_buf(),
            extension: extension.to_string(),
        }),
        several => Err(RenderError::AmbiguousExtension {
            path: path.to_path_buf(),
            extension: extension.to_string(),
            formats: several.iter().map(|w| w.name()).collect(),
        }),
    }
}

/// Validate `table`, render it, and write the result to `path`.
///
/// The format is `format` if given, else inferred from the extension
/// ([`format_for_path`]). Options are a JSON string as in
/// [`render_with_json_options`], with file defaults on top of the format's own:
/// `ansi` writes plain text (`color: false`) unless `color` is set. The bytes are
/// written unchanged (text as UTF-8, no added newline); nothing is written if
/// rendering fails. Returns the format used.
pub fn save(
    table: &Table,
    path: &Path,
    format: Option<&str>,
    options: Option<&str>,
) -> Result<&'static dyn Writer, RenderError> {
    let writer = match format {
        Some(name) => find(name)?,
        None => format_for_path(path)?,
    };
    let mut options = json_options(writer, options)?;
    apply_file_defaults(writer, &mut options);
    let validated = table.validated()?;
    let output = writer.render(&validated, options.as_ref())?;
    std::fs::write(path, output.into_bytes()).map_err(|source| RenderError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(writer)
}

/// Fill in options that differ for files, unless the caller set them. Options that
/// aren't an object are left alone (the writer reports them).
fn apply_file_defaults(writer: &dyn Writer, options: &mut Option<Value>) {
    let defaults: &[(&str, Value)] = match writer.name() {
        "ansi" => &[("color", Value::Bool(false))],
        _ => return,
    };
    let object = match options {
        None | Some(Value::Null) => options.insert(Value::Object(Default::default())),
        Some(v) => v,
    };
    if let Value::Object(map) = object {
        for (key, value) in defaults {
            map.entry(*key).or_insert_with(|| value.clone());
        }
    }
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
