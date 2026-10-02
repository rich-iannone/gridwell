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
    // RGBA pixels. (`as_chunks` is stable since Rust 1.88; xtask is not MSRV-bound.)
    for (pa, pb) in ap.as_chunks::<4>().0.iter().zip(bp.as_chunks::<4>().0) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    /// Write a `w × h` image filled with `base`, with `changed` pixels (row-major from
    /// the top-left) set to `other`, and return its path.
    fn png(
        dir: &Path,
        name: &str,
        w: u32,
        h: u32,
        changed: u32,
        other: [u8; 4],
    ) -> std::path::PathBuf {
        let base = [255, 255, 255, 255];
        let mut img = RgbaImage::from_pixel(w, h, Rgba(base));
        for i in 0..changed {
            img.put_pixel(i % w, i / w, Rgba(other));
        }
        let path = dir.join(name);
        img.save(&path).unwrap();
        path
    }

    fn dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("gridwell-diff-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn missing_golden_is_new() {
        let d = dir();
        let a = png(&d, "new_a.png", 4, 4, 0, [0; 4]);
        assert!(matches!(
            compare(&a, &d.join("absent.png")).unwrap(),
            Comparison::New
        ));
    }

    #[test]
    fn identical_images_are_unchanged() {
        let d = dir();
        let a = png(&d, "same_a.png", 20, 20, 3, [0, 0, 0, 255]);
        let b = png(&d, "same_b.png", 20, 20, 3, [0, 0, 0, 255]);
        assert!(matches!(compare(&a, &b).unwrap(), Comparison::Unchanged));
    }

    #[test]
    fn differences_within_channel_tolerance_are_ignored() {
        let d = dir();
        let a = png(&d, "tol_a.png", 10, 10, 0, [0; 4]);
        // Every pixel off by exactly CHANNEL_TOLERANCE in one channel.
        let t = 255 - CHANNEL_TOLERANCE;
        let b = png(&d, "tol_b.png", 10, 10, 100, [t, 255, 255, 255]);
        assert!(matches!(compare(&a, &b).unwrap(), Comparison::Unchanged));
    }

    #[test]
    fn fraction_threshold_is_respected() {
        // 1000 px; MAX_DIFF_FRACTION (0.5%) allows 5 differing pixels.
        let d = dir();
        let golden = png(&d, "frac_g.png", 100, 10, 0, [0; 4]);
        let at_limit = png(&d, "frac_5.png", 100, 10, 5, [0, 0, 0, 255]);
        let over = png(&d, "frac_6.png", 100, 10, 6, [0, 0, 0, 255]);
        assert!(matches!(
            compare(&at_limit, &golden).unwrap(),
            Comparison::Unchanged
        ));
        match compare(&over, &golden).unwrap() {
            Comparison::Changed { fraction } => {
                assert!((fraction - 0.006).abs() < 1e-9, "{fraction}")
            }
            _ => panic!("expected Changed"),
        }
    }

    #[test]
    fn alpha_channel_counts() {
        let d = dir();
        let a = png(&d, "alpha_a.png", 10, 10, 0, [0; 4]);
        let b = png(&d, "alpha_b.png", 10, 10, 100, [255, 255, 255, 0]);
        assert!(matches!(
            compare(&a, &b).unwrap(),
            Comparison::Changed { .. }
        ));
    }

    #[test]
    fn different_dimensions_are_a_size_mismatch() {
        let d = dir();
        let a = png(&d, "dim_a.png", 10, 10, 0, [0; 4]);
        let b = png(&d, "dim_b.png", 10, 11, 0, [0; 4]);
        assert!(matches!(compare(&a, &b).unwrap(), Comparison::SizeMismatch));
    }

    #[test]
    fn unreadable_image_is_an_error() {
        let d = dir();
        let bad = d.join("bad.png");
        std::fs::write(&bad, b"not a png").unwrap();
        let ok = png(&d, "ok.png", 2, 2, 0, [0; 4]);
        assert!(compare(&bad, &ok).is_err());
    }
}
