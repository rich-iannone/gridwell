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

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Convert {
            input,
            format,
            output,
        } => cmd_convert(input, format, output),
        Command::Validate { input } => cmd_validate(input),
        Command::Formats => cmd_formats(),
    }
}

fn cmd_convert(input: Option<PathBuf>, format: OutputFormat, output: Option<PathBuf>) {
    let json = read_input(input.as_deref());
    let table = parse_table(&json);

    if format.is_binary() {
        let bytes = render_binary(&table, &format);
        let out_path = output.unwrap_or_else(|| {
            eprintln!(
                "Error: binary format '{}' requires --output file",
                format.extension()
            );
            process::exit(1);
        });
        if let Err(e) = fs::write(&out_path, &bytes) {
            eprintln!("Error writing {}: {e}", out_path.display());
            process::exit(1);
        }
        eprintln!("Wrote {} ({} bytes)", out_path.display(), bytes.len());
    } else {
        let text = render_text(&table, &format);
        match output {
            Some(path) => {
                if let Err(e) = fs::write(&path, &text) {
                    eprintln!("Error writing {}: {e}", path.display());
                    process::exit(1);
                }
                eprintln!("Wrote {} ({} bytes)", path.display(), text.len());
            }
            None => {
                let stdout = io::stdout();
                let mut out = stdout.lock();
                out.write_all(text.as_bytes()).unwrap();
            }
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

fn cmd_formats() {
    println!("Supported output formats:");
    println!();
    println!("  Text formats:");
    println!("    html     HTML5 table");
    println!("    latex    LaTeX longtable");
    println!("    typst    Typst table markup");
    println!("    rtf      Rich Text Format");
    println!("    svg      SVG image");
    println!("    ansi     ANSI terminal output");
    println!("    pandoc   Pandoc AST JSON");
    println!("    quarto   Quarto Markdown");
    println!();
    println!("  Binary formats:");
    println!("    docx     Microsoft Word");
    println!("    xlsx     Microsoft Excel");
    println!("    pptx     Microsoft PowerPoint");
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

fn render_text(table: &Table, format: &OutputFormat) -> String {
    let result = match format {
        OutputFormat::Html => gridwell_writer_html::render_html(table).map_err(|e| e.to_string()),
        OutputFormat::Latex => {
            gridwell_writer_latex::render_latex(table).map_err(|e| e.to_string())
        }
        OutputFormat::Typst => {
            gridwell_writer_typst::render_typst(table).map_err(|e| e.to_string())
        }
        OutputFormat::Rtf => gridwell_writer_rtf::render_rtf(table).map_err(|e| e.to_string()),
        OutputFormat::Svg => gridwell_writer_svg::render_svg(table).map_err(|e| e.to_string()),
        OutputFormat::Ansi => gridwell_writer_ansi::render_ansi(table).map_err(|e| e.to_string()),
        OutputFormat::Pandoc => {
            gridwell_writer_pandoc::render_pandoc(table).map_err(|e| e.to_string())
        }
        OutputFormat::Quarto => {
            gridwell_writer_quarto::render_quarto(table).map_err(|e| e.to_string())
        }
        _ => unreachable!(),
    };

    result.unwrap_or_else(|e| {
        eprintln!("Render error: {e}");
        process::exit(1);
    })
}

fn render_binary(table: &Table, format: &OutputFormat) -> Vec<u8> {
    let result = match format {
        OutputFormat::Docx => {
            gridwell_writer_docx::render_docx(table).map_err(|e| e.to_string())
        }
        OutputFormat::Xlsx => {
            gridwell_writer_xlsx::render_xlsx(table).map_err(|e| e.to_string())
        }
        OutputFormat::Pptx => {
            gridwell_writer_pptx::render_pptx(table).map_err(|e| e.to_string())
        }
        _ => unreachable!(),
    };

    result.unwrap_or_else(|e| {
        eprintln!("Render error: {e}");
        process::exit(1);
    })
}
