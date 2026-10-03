//! Is rasterization deterministic? Re-rasterize what `cargo xtask gallery` just
//! rendered, for every format that is not gated yet, and compare each image with
//! the gallery's. A format can be gated only if its images come out the same.
//!
//! The second pass uses its own scratch directory (so a fresh LibreOffice
//! profile and fresh TeX auxiliary files). Each pair of images is classed as
//! pixel-identical, within the gate's tolerance, or different. The gallery's
//! images are also copied to `harness/proposed-goldens/<format>/`, so a format
//! found deterministic can be gated and blessed from the same CI run.
//!
//! This only shows determinism within one run (one machine, one container).
//! Across runs it is shown by the first gated run after blessing.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use gridwell_testkit::examples;

use crate::diff::{self, Comparison};
use crate::formats::{Raster, FORMATS};
use crate::raster::{self, RasterOutcome, Tools};

#[derive(Default)]
struct Tally {
    identical: usize,
    within_tolerance: usize,
    different: Vec<String>,
    failed: Vec<String>,
}

/// Run the probe; returns the Markdown report.
pub fn run(root: &Path) -> Result<String, String> {
    let gallery = root.join("harness/gallery");
    let work_dir = root.join("harness/.work-determinism");
    let second = work_dir.join("img");
    let proposed = root.join("harness/proposed-goldens");
    fs::create_dir_all(&second).map_err(|e| format!("mkdir {}: {e}", second.display()))?;
    let tools = Tools::detect();
    eprintln!("renderers: {}", tools.summary());

    let probed = FORMATS
        .iter()
        .filter(|f| !f.gated && !matches!(f.raster, Raster::AnsiText | Raster::SourceText));
    let mut report = String::new();
    let _ = writeln!(report, "## Rasterization determinism (ungated formats)\n");
    let _ = writeln!(
        report,
        "| format | pixel-identical | within tolerance | different | not rendered |"
    );
    let _ = writeln!(report, "|---|---|---|---|---|");
    let mut details = String::new();
    for fmt in probed {
        let mut tally = Tally::default();
        for ex in examples() {
            let out = gallery
                .join("out")
                .join(format!("{}.{}", ex.name, fmt.ext()));
            let first = gallery
                .join("img")
                .join(format!("{}__{}.png", ex.name, fmt.id));
            if !out.exists() || !first.exists() {
                tally
                    .failed
                    .push(format!("{}: not in the gallery", ex.name));
                continue;
            }
            let again = second.join(format!("{}__{}.png", ex.name, fmt.id));
            match raster::rasterize(fmt.raster, &tools, &out, &again, &work_dir) {
                RasterOutcome::Png(again) => {
                    if identical(&first, &again)? {
                        tally.identical += 1;
                    } else {
                        match diff::compare(&again, &first)? {
                            Comparison::Unchanged => tally.within_tolerance += 1,
                            Comparison::Changed { fraction } => tally.different.push(format!(
                                "{} ({:.2}% of pixels)",
                                ex.name,
                                fraction * 100.0
                            )),
                            Comparison::SizeMismatch => {
                                tally.different.push(format!("{} (size)", ex.name))
                            }
                            Comparison::New => unreachable!("the first image exists"),
                        }
                    }
                    let golden = proposed.join(fmt.id).join(format!("{}.png", ex.name));
                    fs::create_dir_all(golden.parent().unwrap())
                        .map_err(|e| format!("mkdir: {e}"))?;
                    fs::copy(&first, &golden)
                        .map_err(|e| format!("copy {}: {e}", golden.display()))?;
                }
                RasterOutcome::Unavailable(t) => tally.failed.push(format!("{}: no {t}", ex.name)),
                RasterOutcome::Failed(e) => tally.failed.push(format!("{}: {e}", ex.name)),
            }
        }
        let _ = writeln!(
            report,
            "| {} | {} | {} | {} | {} |",
            fmt.id,
            tally.identical,
            tally.within_tolerance,
            tally.different.len(),
            tally.failed.len()
        );
        for (what, list) in [
            ("different", &tally.different),
            ("not rendered", &tally.failed),
        ] {
            if !list.is_empty() {
                let mut shown = list.iter().take(12).cloned().collect::<Vec<_>>().join(", ");
                if list.len() > 12 {
                    let _ = write!(shown, ", … ({} more)", list.len() - 12);
                }
                let _ = writeln!(details, "\n**{} {what}:** {shown}", fmt.id);
            }
        }
    }
    report.push_str(&details);
    Ok(report)
}

/// Same size and the same pixels.
fn identical(a: &Path, b: &Path) -> Result<bool, String> {
    let load = |p: &Path| {
        image::open(p)
            .map(|i| i.to_rgba8())
            .map_err(|e| format!("open {}: {e}", p.display()))
    };
    let (a, b) = (load(a)?, load(b)?);
    Ok(a.dimensions() == b.dimensions() && a.as_raw() == b.as_raw())
}
