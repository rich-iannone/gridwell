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
    /// The registry name (see `gridwell_render::REGISTRY`).
    pub id: &'static str,
    pub raster: Raster,
    /// Whether this format is part of the blocking visual-diff subset.
    pub gated: bool,
}

/// Every format the harness knows about, in gallery column order.
pub const FORMATS: &[Format] = &[
    Format {
        id: "html",
        raster: Raster::Browser,
        gated: true,
    },
    Format {
        id: "svg",
        raster: Raster::Resvg,
        gated: true,
    },
    Format {
        id: "typst",
        raster: Raster::Typst,
        gated: true,
    },
    Format {
        id: "latex",
        raster: Raster::Latex,
        gated: false,
    },
    Format {
        id: "rtf",
        raster: Raster::Office,
        gated: true,
    },
    Format {
        id: "docx",
        raster: Raster::Office,
        gated: true,
    },
    Format {
        id: "xlsx",
        raster: Raster::Office,
        gated: true,
    },
    Format {
        id: "pptx",
        raster: Raster::Office,
        gated: true,
    },
    Format {
        id: "ansi",
        raster: Raster::AnsiText,
        gated: false,
    },
    Format {
        id: "pandoc",
        raster: Raster::SourceText,
        gated: false,
    },
    Format {
        id: "quarto",
        raster: Raster::SourceText,
        gated: false,
    },
];

impl Format {
    /// The file extension, from the format registry.
    pub fn ext(&self) -> &'static str {
        gridwell_render::find(self.id)
            .expect("harness formats are registry names")
            .extension()
    }

    /// Render a table to this format through the registry (which validates it).
    pub fn render(&self, table: &Table) -> Result<Output, String> {
        match gridwell_render::render(table, self.id, None).map_err(|e| e.to_string())? {
            gridwell_render::Output::Text(s) => Ok(Output::Text(s)),
            gridwell_render::Output::Binary(b) => Ok(Output::Bytes(b)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FORMATS;

    #[test]
    fn harness_covers_exactly_the_registry() {
        let mut ours: Vec<&str> = FORMATS.iter().map(|f| f.id).collect();
        let mut registry = gridwell_render::names();
        ours.sort();
        registry.sort();
        assert_eq!(ours, registry);
    }
}
