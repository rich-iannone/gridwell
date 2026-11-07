use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};
use gridwell_ir::Table;
use gridwell_render::conformance::{self, Tier};
use gridwell_render::{find, format_for_path, OutputKind, RenderError, Writer, REGISTRY};

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

        /// Output format (see `gridwell formats`); inferred from the --output
        /// extension when omitted
        #[arg(
            short = 't',
            long = "to",
            value_name = "FORMAT",
            value_parser = parse_format,
            required_unless_present = "output"
        )]
        format: Option<&'static dyn Writer>,

        /// Output file (defaults to stdout for text, required for binary). Files
        /// get file defaults: `ansi` writes plain text unless `color` is set
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

    /// Validate a table IR JSON file, and warn of features a format can't express
    Validate {
        /// Input file (use "-" or omit for stdin)
        #[arg(value_name = "INPUT")]
        input: Option<PathBuf>,

        /// Warn of what this format can't express (default: every format)
        #[arg(short = 't', long = "to", value_name = "FORMAT", value_parser = parse_format)]
        format: Option<&'static dyn Writer>,
    },

    /// Normalize a table IR to a conformance grid, or check one against an
    /// expected grid
    #[command(subcommand)]
    Conformance(ConformanceCommand),

    /// List supported output formats
    Formats {
        /// Print machine-readable JSON, including each format's options and defaults
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ConformanceCommand {
    /// Print a table IR's normalized grid (JSON): what an expected grid is made of
    Normalize {
        /// Input file (use "-" or omit for stdin)
        #[arg(value_name = "INPUT")]
        input: Option<PathBuf>,

        /// The tier to write the grid to: L1 structure, L2 content, L3 styling
        #[arg(long, value_name = "TIER", default_value = "L3", value_parser = parse_tier)]
        tier: Tier,
    },

    /// Check a table IR against an expected grid (exit 1 when it doesn't conform)
    Check {
        /// The table IR (use "-" for stdin)
        #[arg(value_name = "INPUT")]
        input: PathBuf,

        /// The expected grid
        #[arg(value_name = "EXPECTED")]
        expected: PathBuf,

        /// The tier to check to (default: the expected grid's)
        #[arg(long, value_name = "TIER", value_parser = parse_tier)]
        tier: Option<Tier>,

        /// Print the report as JSON
        #[arg(long)]
        json: bool,
    },
}

fn parse_tier(name: &str) -> Result<Tier, String> {
    name.parse()
        .map_err(|e: conformance::ConformanceError| e.to_string())
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
        Command::Validate { input, format } => cmd_validate(input, format),
        Command::Conformance(ConformanceCommand::Normalize { input, tier }) => {
            cmd_normalize(input, tier)
        }
        Command::Conformance(ConformanceCommand::Check {
            input,
            expected,
            tier,
            json,
        }) => cmd_check(input, expected, tier, json),
        Command::Formats { json } => cmd_formats(json),
    }
}

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("Error: {msg}");
    process::exit(1);
}

fn cmd_convert(
    input: Option<PathBuf>,
    format: Option<&'static dyn Writer>,
    output: Option<PathBuf>,
    options: Option<String>,
    options_file: Option<PathBuf>,
) {
    // Check arguments before doing any work. Clap ensures --to or --output.
    let writer = match (format, &output) {
        (Some(w), _) => w,
        (None, Some(path)) => format_for_path(path).unwrap_or_else(|e| fail(with_to_hint(e))),
        (None, None) => unreachable!("clap requires --to without --output"),
    };
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
    // The table is validated with the rest below; a warning needs a valid one.
    let warnings = gridwell_render::warnings(&table, Some(writer.name())).unwrap_or_default();

    // The registry validates the IR (writers never see invalid tables) and the
    // options, and writes nothing if either fails.
    match output {
        Some(path) => {
            gridwell_render::save(&table, &path, Some(writer.name()), options.as_deref())
                .unwrap_or_else(|e| fail(e));
            let len = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            eprintln!("Wrote {} ({len} bytes, {})", path.display(), writer.name());
        }
        None => {
            let rendered = gridwell_render::render_with_json_options(
                &table,
                writer.name(),
                options.as_deref(),
            )
            .unwrap_or_else(|e| fail(e));
            let stdout = io::stdout();
            let mut out = stdout.lock();
            out.write_all(&rendered.into_bytes()).unwrap();
        }
    }
    for w in &warnings {
        eprintln!("Warning: {w}");
    }
}

/// Extension errors say "pass a format"; on the command line that's --to.
fn with_to_hint(e: RenderError) -> String {
    format!("{e} with --to")
}

fn cmd_validate(input: Option<PathBuf>, format: Option<&'static dyn Writer>) {
    let json = read_input(input.as_deref());
    let table = parse_table(&json);

    let errors = table.validate();
    if errors.is_empty() {
        eprintln!("Valid: no errors found.");
        // Warnings don't fail validation: the table renders, without them.
        let warnings =
            gridwell_render::warnings(&table, format.map(|w| w.name())).unwrap_or_else(|e| fail(e));
        if !warnings.is_empty() {
            eprintln!("{} warning(s):", warnings.len());
            for w in &warnings {
                eprintln!("  - {w}");
            }
        }
    } else {
        eprintln!("Found {} validation error(s):", errors.len());
        for err in &errors {
            eprintln!("  - {err}");
        }
        if let Some(note) = table.ensure_valid().err().and_then(|e| e.version_note()) {
            eprintln!("{note}");
        }
        process::exit(1);
    }
}

fn cmd_normalize(input: Option<PathBuf>, tier: Tier) {
    let json = read_input(input.as_deref());
    let table = parse_table(&json);
    let validated = table.validated().unwrap_or_else(|e| fail(e));
    let grid = conformance::normalize(&validated, tier);
    println!("{}", serde_json::to_string_pretty(&grid).unwrap());
}

fn cmd_check(input: PathBuf, expected: PathBuf, tier: Option<Tier>, json: bool) {
    let ir = read_input(Some(&input));
    let grid = fs::read_to_string(&expected)
        .unwrap_or_else(|e| fail(format_args!("reading {}: {e}", expected.display())));
    let report = conformance::check_json(&ir, &grid, tier).unwrap_or_else(|e| fail(e));
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report.to_json()).unwrap()
        );
    } else {
        print!("{report}");
    }
    if !report.passed() {
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
                    "experimental": w.experimental(),
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
                "    {:<8} {} (.{}{})",
                w.name(),
                w.description(),
                w.extension(),
                if w.experimental() {
                    ", experimental"
                } else {
                    ""
                }
            );
        }
    }
    println!();
    if REGISTRY.iter().any(|w| w.experimental()) {
        println!("Experimental formats write valid output with every cell, but their styling");
        println!("is best-effort and may change between releases.");
    }
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
