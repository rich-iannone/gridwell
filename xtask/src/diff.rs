//! Comparing freshly rendered PNGs against committed golden images for the
//! gated (deterministic) formats.

use std::path::Path;

/// Fraction of pixels allowed to differ (beyond [`CHANNEL_TOLERANCE`]) before a
/// comparison counts as a regression. Small non-determinism (antialiasing)
/// stays under this; a real visual change blows past it.
const MAX_DIFF_FRACTION: f64 = 0.005;
/// Per-channel absolute difference under which two pixels are "the same".
const CHANNEL_TOLERANCE: u8 = 12;

/// The result of comparing one rendered image to its golden.
pub enum Comparison {
    /// No golden on disk yet — this is a new image.
    New,
    /// Within tolerance of the golden.
    Unchanged,
    /// Differs from the golden; carries the fraction of differing pixels.
    Changed { fraction: f64 },
    /// Golden and render have different dimensions.
    SizeMismatch,
}

/// Compare a rendered PNG to a golden PNG (if present).
pub fn compare(render: &Path, golden: &Path) -> Result<Comparison, String> {
    if !golden.exists() {
        return Ok(Comparison::New);
    }
    let a = load(render)?;
    let b = load(golden)?;
    if a.dimensions() != b.dimensions() {
        return Ok(Comparison::SizeMismatch);
    }
    let (w, h) = a.dimensions();
    let total = (w as u64) * (h as u64);
    if total == 0 {
        return Ok(Comparison::Unchanged);
    }
    let ap = a.as_raw();
    let bp = b.as_raw();
    let mut differing: u64 = 0;
    for (pa, pb) in ap.chunks_exact(4).zip(bp.chunks_exact(4)) {
        let d = pa
            .iter()
            .zip(pb.iter())
            .any(|(x, y)| x.abs_diff(*y) > CHANNEL_TOLERANCE);
        if d {
            differing += 1;
        }
    }
    let fraction = differing as f64 / total as f64;
    Ok(if fraction > MAX_DIFF_FRACTION {
        Comparison::Changed { fraction }
    } else {
        Comparison::Unchanged
    })
}

fn load(path: &Path) -> Result<image::RgbaImage, String> {
    let img = image::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    Ok(img.to_rgba8())
}
