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

