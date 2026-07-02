//! Gridwell visual test harness.
//!
//! - `cargo xtask corpus`  — serialize the example registry to `harness/corpus/*.json`
//! - `cargo xtask gallery` — render every example to every format and build the
//!   browsable gallery at `harness/gallery/index.html`
//! - `cargo xtask gallery --check`  — additionally diff the gated (deterministic)
//!   formats against `harness/goldens` and exit non-zero on a regression
//! - `cargo xtask gallery --accept` — update the goldens from the current render

mod diff;
mod formats;
mod gallery;
mod raster;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use gridwell_testkit::examples;

#[derive(Parser)]
#[command(name = "xtask", about = "Gridwell visual test harness")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Serialize the example corpus to harness/corpus/*.json
    Corpus,
    /// Render every example to every format and build harness/gallery/index.html
    Gallery {
        /// Compare gated formats to goldens; exit non-zero on regression
        #[arg(long)]
        check: bool,
        /// Update goldens for gated formats from the current render
        #[arg(long)]
        accept: bool,
    },
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask is a workspace member")
        .to_path_buf()
}

fn main() {
    let cli = Cli::parse();
    let root = workspace_root();

    let exit = match cli.command {
        Command::Corpus => match run_corpus(&root) {
            Ok(n) => {
                eprintln!("wrote {n} corpus files to harness/corpus");
                0
            }
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
        Command::Gallery { check, accept } => match gallery::run(&root, check, accept) {
            Ok(regressions) => {
                if regressions {
                    2
                } else {
                    0
                }
            }
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
    };
    std::process::exit(exit);
}

fn run_corpus(root: &std::path::Path) -> Result<usize, String> {
    let dir = root.join("harness/corpus");
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    let mut count = 0;
    for ex in examples() {
        let json = ex
            .table()
            .to_json()
            .map_err(|e| format!("serialize {}: {e}", ex.name))?;
        let path = dir.join(format!("{}.json", ex.name));
        std::fs::write(&path, json).map_err(|e| format!("write {}: {e}", path.display()))?;
        count += 1;
    }
    Ok(count)
}
