use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};
use gridwell_ir::Table;
use gridwell_render::{find, OutputKind, Writer, REGISTRY};

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

        /// Output format (see `gridwell formats`)
        #[arg(short = 't', long = "to", value_name = "FORMAT", value_parser = parse_format)]
        format: &'static dyn Writer,

        /// Output file (defaults to stdout for text, required for binary)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,

        /// Writer options as a JSON object, e.g. '{"inline_styles": true}'
        #[arg(
            short = 'O',
            long = "options",
            value_name = "JSON",
            conflicts_with = "options_file"
        )]
        options: Option<String>,

        /// Read writer options from a JSON file
        #[arg(long = "options-file", value_name = "FILE")]
        options_file: Option<PathBuf>,
    },

    /// Validate a table IR JSON file
    Validate {
        /// Input file (use "-" or omit for stdin)
        #[arg(value_name = "INPUT")]
        input: Option<PathBuf>,
    },

    /// List supported output formats
    Formats {
        /// Print machine-readable JSON, including each format's options and defaults
        #[arg(long)]
        json: bool,
    },
}

fn parse_format(name: &str) -> Result<&'static dyn Writer, String> {
    find(name).map_err(|e| e.to_string())
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Convert {
            input,
            format,
            output,
            options,
            options_file,
        } => cmd_convert(input, format, output, options, options_file),
        Command::Validate { input } => cmd_validate(input),
        Command::Formats { json } => cmd_formats(json),
    }
}

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("Error: {msg}");
    process::exit(1);
}

fn cmd_convert(
    input: Option<PathBuf>,
    writer: &'static dyn Writer,
    output: Option<PathBuf>,
    options: Option<String>,
    options_file: Option<PathBuf>,
) {
    // Check arguments before doing any work.
    if writer.kind() == OutputKind::Binary && output.is_none() {
        fail(format_args!(
            "binary format '{}' requires --output file",
            writer.name()
        ));
    }
    let options = match options_file {
        Some(path) => Some(
            fs::read_to_string(&path)
                .unwrap_or_else(|e| fail(format_args!("reading {}: {e}", path.display()))),
        ),
        None => options,
    };

    let json = read_input(input.as_deref());
    let table = parse_table(&json);

    // The registry validates the IR (writers never see invalid tables) and the
    // options.
    let rendered =
        gridwell_render::render_with_json_options(&table, writer.name(), options.as_deref())
            .unwrap_or_else(|e| fail(e));

    match output {
        Some(path) => {
            let bytes = rendered.into_bytes();
            if let Err(e) = fs::write(&path, &bytes) {
                fail(format_args!("writing {}: {e}", path.display()));
            }
            eprintln!("Wrote {} ({} bytes)", path.display(), bytes.len());
        }
        None => {
            let stdout = io::stdout();
            let mut out = stdout.lock();
            out.write_all(&rendered.into_bytes()).unwrap();
        }
    }
}

fn cmd_validate(input: Option<PathBuf>) {
    let json = read_input(input.as_deref());
    let table = parse_table(&json);

    let errors = table.validate();
    if errors.is_empty() {
        eprintln!("Valid: no errors found.");
    } else {
        eprintln!("Found {} validation error(s):", errors.len());
        for err in &errors {
            eprintln!("  - {err}");
        }
        process::exit(1);
    }
}

fn cmd_formats(json: bool) {
    if json {
        let list: Vec<serde_json::Value> = REGISTRY
            .iter()
            .map(|w| {
                serde_json::json!({
                    "name": w.name(),
                    "description": w.description(),
                    "kind": w.kind().to_string(),
                    "extension": w.extension(),
                    "media_type": w.media_type(),
                    "options": w.default_options(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&list).unwrap());
        return;
    }
    println!("Supported output formats:");
    for kind in [OutputKind::Text, OutputKind::Binary] {
        println!();
        println!(
            "  {} formats:",
            if kind == OutputKind::Text {
                "Text"
            } else {
                "Binary"
            }
        );
        for w in REGISTRY.iter().filter(|w| w.kind() == kind) {
            println!(
                "    {:<8} {} (.{})",
                w.name(),
                w.description(),
                w.extension()
            );
        }
    }
    println!();
    println!("Pass writer options with --options '<JSON>'; see `gridwell formats --json`.");
}

fn read_input(path: Option<&std::path::Path>) -> String {
    match path {
        Some(p) if p.to_str() != Some("-") => fs::read_to_string(p).unwrap_or_else(|e| {
            eprintln!("Error reading {}: {e}", p.display());
            process::exit(1);
        }),
        _ => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf).unwrap_or_else(|e| {
                eprintln!("Error reading stdin: {e}");
                process::exit(1);
            });
            buf
        }
    }
}

fn parse_table(json: &str) -> Table {
    Table::from_json(json).unwrap_or_else(|e| {
        eprintln!("Error parsing IR: {e}");
        process::exit(1);
    })
}
