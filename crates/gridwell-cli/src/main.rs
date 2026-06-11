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

