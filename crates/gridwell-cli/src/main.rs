use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand, ValueEnum};
use gridwell_ir::Table;

#[derive(Parser)]
#[command(
    name = "gridwell",
    about = "Fast multi-format table rendering from a declarative IR",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Convert a table IR JSON file to an output format
    Convert {
        /// Input file (use "-" or omit for stdin)
        #[arg(value_name = "INPUT")]
        input: Option<PathBuf>,

        /// Output format
        #[arg(short = 't', long = "to", value_name = "FORMAT")]
        format: OutputFormat,

        /// Output file (defaults to stdout for text, required for binary)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
    },

    /// Validate a table IR JSON file
    Validate {
        /// Input file (use "-" or omit for stdin)
        #[arg(value_name = "INPUT")]
        input: Option<PathBuf>,
    },

    /// List supported output formats
    Formats,
}

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    Html,
    Latex,
    Typst,
    Rtf,
    Svg,
    Ansi,
    Pandoc,
    Quarto,
    Docx,
    Xlsx,
    Pptx,
}

impl OutputFormat {
    fn is_binary(&self) -> bool {
        matches!(self, Self::Docx | Self::Xlsx | Self::Pptx)
    }

    fn extension(&self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Latex => "tex",
            Self::Typst => "typ",
            Self::Rtf => "rtf",
            Self::Svg => "svg",
            Self::Ansi => "txt",
            Self::Pandoc => "json",
            Self::Quarto => "qmd",
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
            Self::Pptx => "pptx",
        }
    }
}

