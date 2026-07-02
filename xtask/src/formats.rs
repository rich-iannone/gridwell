//! The output formats the harness renders, plus how each is turned into an
//! image (or shown as text) in the gallery.

use gridwell_ir::Table;

/// A rendered output: text formats produce a string, binary formats bytes.
pub enum Output {
    Text(String),
    Bytes(Vec<u8>),
}

/// How a format's output is turned into a preview in the gallery.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Raster {
    /// HTML rendered by headless Chromium.
    Browser,
    /// SVG rendered by resvg.
    Resvg,
    /// Typst source compiled to PNG.
    Typst,
    /// LaTeX compiled with xelatex, then pdftoppm.
    Latex,
    /// RTF/OOXML converted by LibreOffice, then pdftoppm.
    Office,
    /// ANSI terminal text, shown as escaped text.
    AnsiText,
    /// Plain source text (Pandoc AST / Quarto markdown), shown verbatim.
    SourceText,
}

/// One output format.
#[derive(Clone, Copy)]
pub struct Format {
    pub id: &'static str,
    pub ext: &'static str,
    pub raster: Raster,
    /// Whether this format is part of the blocking visual-diff subset.
    pub gated: bool,
}

/// Every format the harness knows about, in gallery column order.
pub const FORMATS: &[Format] = &[
    Format {
        id: "html",
        ext: "html",
        raster: Raster::Browser,
        gated: true,
    },
    Format {
        id: "svg",
        ext: "svg",
        raster: Raster::Resvg,
        gated: true,
    },
    Format {
        id: "typst",
        ext: "typ",
        raster: Raster::Typst,
        gated: true,
    },
    Format {
        id: "latex",
        ext: "tex",
        raster: Raster::Latex,
        gated: false,
    },
    Format {
        id: "rtf",
        ext: "rtf",
        raster: Raster::Office,
        gated: false,
    },
    Format {
        id: "docx",
        ext: "docx",
        raster: Raster::Office,
        gated: false,
    },
    Format {
        id: "xlsx",
        ext: "xlsx",
        raster: Raster::Office,
        gated: false,
    },
    Format {
        id: "pptx",
        ext: "pptx",
        raster: Raster::Office,
        gated: false,
    },
    Format {
        id: "ansi",
        ext: "txt",
        raster: Raster::AnsiText,
        gated: false,
    },
    Format {
        id: "pandoc",
        ext: "json",
        raster: Raster::SourceText,
        gated: false,
    },
    Format {
        id: "quarto",
        ext: "qmd",
        raster: Raster::SourceText,
        gated: false,
    },
];

impl Format {
    /// Render a table to this format.
    pub fn render(&self, table: &Table) -> Result<Output, String> {
        fn text<E: std::fmt::Display>(r: Result<String, E>) -> Result<Output, String> {
            r.map(Output::Text).map_err(|e| e.to_string())
        }
        fn bytes<E: std::fmt::Display>(r: Result<Vec<u8>, E>) -> Result<Output, String> {
            r.map(Output::Bytes).map_err(|e| e.to_string())
        }
        match self.id {
            "html" => text(gridwell_writer_html::render_html(table)),
            "svg" => text(gridwell_writer_svg::render_svg(table)),
            "typst" => text(gridwell_writer_typst::render_typst(table)),
            "latex" => text(gridwell_writer_latex::render_latex(table)),
            "rtf" => text(gridwell_writer_rtf::render_rtf(table)),
            "ansi" => text(gridwell_writer_ansi::render_ansi(table)),
            "pandoc" => text(gridwell_writer_pandoc::render_pandoc(table)),
            "quarto" => text(gridwell_writer_quarto::render_quarto(table)),
            "docx" => bytes(gridwell_writer_docx::render_docx(table)),
            "xlsx" => bytes(gridwell_writer_xlsx::render_xlsx(table)),
            "pptx" => bytes(gridwell_writer_pptx::render_pptx(table)),
            other => Err(format!("unknown format {other}")),
        }
    }
}
