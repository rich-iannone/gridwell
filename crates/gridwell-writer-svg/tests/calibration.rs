//! Calibrate the SVG text-width estimate against real rendering.
//!
//! Probe strings are rendered with `rsvg-convert` in the writer's default font stack
//! at the sizes the writer actually uses (superscript, note, body, subtitle, title)
//! plus a large size, at 1× and 2× zoom (the harness renders at 2×). Each string's
//! ink width is measured from the PNG. The estimate must never be narrower than the
//! ink, or laid-out text would overprint neighbouring cells or run off the canvas.
//! Small sizes matter most: hinting rounds glyph advances to whole pixels.
//!
//! Fonts differ by machine, which is the point: run locally (e.g. macOS Arial) and in
//! the pinned harness image (Noto / DejaVu). Skipped when `rsvg-convert` is missing
//! unless `GRIDWELL_REQUIRE_RSVG` is set.

use std::process::Command;

use gridwell_writer_svg::measure::{text_width, SUP_SCALE};
use gridwell_writer_svg::SvgConfig;

fn require_rsvg() -> bool {
    if Command::new("rsvg-convert")
        .arg("--version")
        .output()
        .is_ok()
    {
        return true;
    }
    if std::env::var_os("GRIDWELL_REQUIRE_RSVG").is_some_and(|v| !v.is_empty()) {
        panic!("rsvg-convert not found but GRIDWELL_REQUIRE_RSVG is set");
    }
    eprintln!("skipping: rsvg-convert not found");
    false
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn probes() -> Vec<String> {
    let mut probes: Vec<String> = (0x21u8..0x7f)
        .map(|b| (b as char).to_string().repeat(8))
        .collect();
    probes.extend(
        [
            "Includes bonus points.",
            "clamped between 80 and 200px",
            "https://example.com/very/long/path/that/should/not/break/nicely?query=1",
            "WWWWMMMM@@@@%%%%",
            "Regional Sales Performance",
            "Fiscal years 2023–2024 (in millions USD)",
            "1,250.3  $1,200  12.5%  ±0.01",
            "café naïve Ærø Ελληνικά Кириллица",
            "販売実績 東京 大阪",
            "مرحبا שלום",
            "A systems language.",
            "Measured under pseudo-first-order conditions.",
            "iiiiiiiiiiiiiiii llllllllllllllll",
        ]
        .map(String::from),
    );
    probes
}

/// Render every probe as its own band of one SVG; return each probe's ink width in
/// CSS px (`None` for blank bands).
fn ink_widths(probes: &[String], size: f64, bold: bool, zoom: u32, tag: &str) -> Vec<Option<f64>> {
    let family = SvgConfig::default().font_family;
    let weight = if bold { " font-weight=\"bold\"" } else { "" };
    let band = (size * 2.0).ceil();
    let longest = probes
        .iter()
        .map(|p| text_width(p, size, bold))
        .fold(0.0, f64::max);
    let width = (40.0 + longest * 1.5).ceil();
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{h}\">",
        h = band * probes.len() as f64
    );
    for (i, p) in probes.iter().enumerate() {
        svg += &format!(
            "<text x=\"20\" y=\"{y}\" font-family=\"{family}\" font-size=\"{size}px\"{weight} \
             xml:space=\"preserve\">{t}</text>",
            y = band * i as f64 + size * 1.3,
            t = escape(p),
        );
    }
    svg += "</svg>";

    let dir = std::env::temp_dir().join(format!("gridwell-svg-cal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let svg_path = dir.join(format!("{tag}.svg"));
    let png_path = dir.join(format!("{tag}.png"));
    std::fs::write(&svg_path, svg).unwrap();
    let out = Command::new("rsvg-convert")
        .args([
            "--zoom",
            &zoom.to_string(),
            "--background-color",
            "white",
            "-o",
        ])
        .arg(&png_path)
        .arg(&svg_path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "rsvg failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let img = image::open(&png_path).unwrap().to_rgba8();
    let band_px = (band * zoom as f64) as u32;
    (0..probes.len() as u32)
        .map(|i| {
            let (mut min_x, mut max_x) = (u32::MAX, 0u32);
            for y in i * band_px..((i + 1) * band_px).min(img.height()) {
                for x in 0..img.width() {
                    if img.get_pixel(x, y).0[..3].iter().any(|&c| c < 200) {
                        min_x = min_x.min(x);
                        max_x = max_x.max(x);
                    }
                }
            }
            (min_x <= max_x).then(|| (max_x - min_x + 1) as f64 / zoom as f64)
        })
        .collect()
}

#[test]
fn estimate_is_never_narrower_than_rendered_ink() {
    if !require_rsvg() {
        return;
    }
    let probes = probes();
    let body = SvgConfig::default().font_size;
    // Superscript marks, notes, body, subtitle, title, and a large size.
    let sizes = [
        body * 0.85 * SUP_SCALE,
        body * 0.85,
        body,
        body * 1.1,
        body * 1.4,
        50.0,
    ];

    let mut failures = Vec::new();
    let mut tightest = (f64::INFINITY, String::new());
    for &size in &sizes {
        for zoom in [1, 2] {
            for bold in [false, true] {
                // Batches keep each rendered sheet a sensible size at 50px / 2×.
                let inks: Vec<Option<f64>> = probes
                    .chunks(16)
                    .enumerate()
                    .flat_map(|(i, batch)| {
                        let tag = format!("s{size:.2}-z{zoom}-b{bold}-{i}");
                        ink_widths(batch, size, bold, zoom, &tag)
                    })
                    .collect();
                for (probe, ink) in probes.iter().zip(inks) {
                    let Some(ink) = ink else { continue };
                    let est = text_width(probe, size, bold);
                    let ctx = format!("{probe:?} size={size:.2} zoom={zoom} bold={bold}");
                    if est / ink < tightest.0 {
                        tightest = (est / ink, ctx.clone());
                    }
                    // Half a device pixel of antialiasing slack.
                    if est + 0.5 < ink {
                        failures.push(format!("{ctx}: estimate {est:.1}px < ink {ink:.1}px"));
                    }
                }
            }
        }
    }
    eprintln!(
        "tightest estimate/ink ratio: {:.3} for {}",
        tightest.0, tightest.1
    );
    assert!(
        failures.is_empty(),
        "estimate too narrow ({}):\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}
