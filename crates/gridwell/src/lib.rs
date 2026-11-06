//! Render one table, described once in gridwell's IR (JSON), to HTML, LaTeX,
//! Typst, RTF, SVG, the terminal, the Pandoc and Quarto ASTs, and Word, Excel and
//! PowerPoint files.
//!
//! This crate gathers the others:
//!
//! - the format registry, at the crate root: [`render`], [`save`], [`warnings`],
//!   [`find`], [`REGISTRY`] and the rest of `gridwell-render`;
//! - the IR: [`Table`], and everything else in [`ir`];
//! - the resolved layout every writer renders, in [`layout`];
//! - each writer, named as its format is ([`html`], [`latex`], …), for its typed
//!   options and its own render functions.
//!
//! ```
//! # let json = r#"{"ir_version":"1.0","config":{"table_cols":1,"body_rows":1},
//! #   "styles":{"defs":{},"compositions":{},"conditionals":[]},
//! #   "column_spec":[{"id":"a"}],
//! #   "table":{"thead":{"rows":[]},"tbody":[{"rows":[{"cells":[{"content":[]}]}]}]}}"#;
//! let table = gridwell::Table::from_json(json).unwrap();
//! let options = serde_json::json!({ "inline_styles": true });
//! let html = gridwell::render(&table, "html", Some(&options)).unwrap();
//! assert!(html.as_text().unwrap().contains("<table"));
//! ```
//!
//! # Features
//!
//! One per format, named as the format is: `html`, `latex`, `typst`, `rtf`, `svg`,
//! `ansi`, `pandoc`, `quarto`, `docx`, `xlsx` and `pptx`. Every one is on by
//! default. A format whose feature is off isn't in [`REGISTRY`] (asking for it is
//! a [`RenderError::UnknownFormat`]) and its writer isn't built, so
//!
//! ```toml
//! gridwell = { version = "0.1", default-features = false, features = ["html"] }
//! ```
//!
//! builds the HTML writer alone.

pub use gridwell_ir as ir;
pub use gridwell_ir::{InvalidTable, ParseError, Table, ValidatedTable};
pub use gridwell_layout as layout;
pub use gridwell_render::*;

#[cfg(feature = "ansi")]
pub use gridwell_writer_ansi as ansi;
#[cfg(feature = "docx")]
pub use gridwell_writer_docx as docx;
#[cfg(feature = "html")]
pub use gridwell_writer_html as html;
#[cfg(feature = "latex")]
pub use gridwell_writer_latex as latex;
#[cfg(feature = "pandoc")]
pub use gridwell_writer_pandoc as pandoc;
#[cfg(feature = "pptx")]
pub use gridwell_writer_pptx as pptx;
#[cfg(feature = "quarto")]
pub use gridwell_writer_quarto as quarto;
#[cfg(feature = "rtf")]
pub use gridwell_writer_rtf as rtf;
#[cfg(feature = "svg")]
pub use gridwell_writer_svg as svg;
#[cfg(feature = "typst")]
pub use gridwell_writer_typst as typst;
#[cfg(feature = "xlsx")]
pub use gridwell_writer_xlsx as xlsx;
