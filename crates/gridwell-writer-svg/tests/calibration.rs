//! Calibrate the SVG text widths against real rendering with `rsvg-convert`.
//!
//! librsvg ignores `textLength`, so it shows what a fallback font really draws.
//! Probe strings are rendered in the writer's default font list at the sizes the
//! writer uses, regular/bold × upright/italic, at 1× and 2× zoom, and each string's
//! ink width is measured from the PNG. Two claims:
//!
//! 1. **Room** (always): whatever font rsvg resolves from the list (Roboto, or an
//!    Arial-compatible fallback), the ink fits in the room the layout leaves
//!    (a line's width from [`wrap`]: [`text_width`] plus italic overhang), so text
//!    never overprints a neighboring cell. This covers
//!    characters outside the metric tables too (CJK, emoji, RTL), whose widths are
//!    estimates.
//! 2. **Accuracy** (when Roboto is installed; required if `GRIDWELL_REQUIRE_ROBOTO`
//!    is set): for text the tables cover, the ink matches Roboto's width
//!    ([`draw_width`], what `textLength` draws in browsers), allowing for side
//!    bearings.
//!
//! Skipped when `rsvg-convert` is missing unless `GRIDWELL_REQUIRE_RSVG` is set.

use std::process::Command;

use gridwell_writer_svg::measure::{
    draw_width, roboto_em, text_width, wrap, Run, RunStyle, SUP_SCALE,
};
use gridwell_writer_svg::SvgConfig;

fn require_rsvg() -> bool {
    if Command::new("rsvg-convert")
        .arg("--version")
        .output()
        .is_ok()
    {
        return true;
    }
    if required("GRIDWELL_REQUIRE_RSVG") {
        panic!("rsvg-convert not found but GRIDWELL_REQUIRE_RSVG is set");
    }
    eprintln!("skipping: rsvg-convert not found");
    false
}

fn required(var: &str) -> bool {
    std::env::var_os(var).is_some_and(|v| !v.is_empty())
}

/// Whether fontconfig has Roboto (what rsvg uses on Linux).
fn roboto_installed() -> bool {
    Command::new("fc-list")
        .args([":family=Roboto", "family"])
        .output()
        .is_ok_and(|o| !o.stdout.is_empty())
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
            "1,250.3 $1,200 12.5% ±0.01",
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
fn ink_widths(
    probes: &[String],
    size: f64,
    (bold, italic): (bool, bool),
    zoom: u32,
    tag: &str,
) -> Vec<Option<f64>> {
    let family = SvgConfig::default().font_family.replace('\'', "&apos;");
    let weight = if bold { " font-weight=\"bold\"" } else { "" };
    let slant = if italic { " font-style=\"italic\"" } else { "" };
    let band = (size * 2.0).ceil();
    let longest = probes
        .iter()
        .map(|p| text_width(p, size, bold, italic))
        .fold(0.0, f64::max);
    let width = (40.0 + longest * 1.5).ceil();
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{h}\">",
        h = band * probes.len() as f64
    );
    for (i, p) in probes.iter().enumerate() {
        svg += &format!(
            "<text x=\"20\" y=\"{y}\" font-family=\"{family}\" font-size=\"{size}px\"{weight}{slant} \
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

/// One rendered probe: `(probe, size, (bold, italic), zoom, ink width)`.
type Measurement = (String, f64, (bool, bool), u32, f64);

/// Every probe's ink width at every size, style and zoom, rendered once and shared
/// by both tests.
fn measurements() -> &'static [Measurement] {
    static CACHE: std::sync::OnceLock<Vec<Measurement>> = std::sync::OnceLock::new();
    CACHE.get_or_init(render_all)
}

fn render_all() -> Vec<Measurement> {
    let probes = probes();
    let body = SvgConfig::default().font_size;
    // Superscript marks, notes, body, subtitle, title, and a large size.
    let sizes = [
        body * 0.85 * SUP_SCALE,
        body * 0.85,
        body * 0.9,
        body,
        body * 1.25,
        50.0,
    ];
    let mut out = Vec::new();
    for &size in &sizes {
        for zoom in [1, 2] {
            for style in [(false, false), (true, false), (false, true), (true, true)] {
                // Batches keep each rendered sheet a sensible size at 50px / 2×.
                let inks: Vec<Option<f64>> = probes
                    .chunks(16)
                    .enumerate()
                    .flat_map(|(i, batch)| {
                        let tag = format!("s{size:.2}-z{zoom}-b{}-i{}-{i}", style.0, style.1);
                        ink_widths(batch, size, style, zoom, &tag)
                    })
                    .collect();
                for (probe, ink) in probes.iter().zip(inks) {
                    if let Some(ink) = ink {
                        out.push((probe.clone(), size, style, zoom, ink));
                    }
                }
            }
        }
    }
    out
}

#[test]
fn rendered_text_fits_the_room_the_layout_leaves() {
    if !require_rsvg() {
        return;
    }
    let mut failures = Vec::new();
    let mut tightest = (f64::INFINITY, String::new());
    for &(ref probe, size, (bold, italic), zoom, ink) in measurements() {
        // The room the layout gives the text as one line (its natural width).
        let run = Run {
            text: probe.clone(),
            style: RunStyle {
                bold,
                italic,
                ..RunStyle::default()
            },
        };
        let room = wrap(&[run], size, f64::INFINITY)[0].width;
        let ctx = format!("{probe:?} size={size:.2} zoom={zoom} bold={bold} italic={italic}");
        if room / ink < tightest.0 {
            tightest = (room / ink, ctx.clone());
        }
        // Ink is measured in whole device pixels (antialiased edges included).
        if room + 1.0 / (zoom as f64) < ink {
            failures.push(format!("{ctx}: room {room:.1}px < ink {ink:.1}px"));
        }
    }
    eprintln!(
        "tightest room/ink ratio: {:.3} for {}",
        tightest.0, tightest.1
    );
    assert!(
        failures.is_empty(),
        "text wider than its room ({}):\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

#[test]
fn roboto_renders_at_the_width_text_is_drawn_at() {
    if !require_rsvg() {
        return;
    }
    if !roboto_installed() {
        if required("GRIDWELL_REQUIRE_ROBOTO") {
            panic!("Roboto not found (fc-list) but GRIDWELL_REQUIRE_ROBOTO is set");
        }
        eprintln!("skipping: Roboto not installed");
        return;
    }
    let mut failures = Vec::new();
    let mut checked = 0;
    for &(ref probe, size, (bold, italic), zoom, ink) in measurements() {
        // Only text the Roboto table covers (the rest is estimated).
        if !probe.chars().all(|c| roboto_em(c, bold, italic).is_some()) {
            continue;
        }
        checked += 1;
        let draw = draw_width(probe, size, bold, italic);
        let ctx = format!("{probe:?} size={size:.2} zoom={zoom} bold={bold} italic={italic}");
        // Ink runs from the first glyph's ink to the last's: the advance sum less
        // two side bearings (up to ~0.15em each; italics can overhang), less a
        // little kerning. Never wider than the advances, give or take a device
        // pixel of hinting and antialiasing per side.
        let slack = 2.0 / zoom as f64;
        if ink > draw + slack {
            failures.push(format!("{ctx}: ink {ink:.1}px > Roboto width {draw:.1}px"));
        }
        if ink < draw * 0.97 - 0.3 * size - slack {
            failures.push(format!(
                "{ctx}: ink {ink:.1}px far below Roboto width {draw:.1}px"
            ));
        }
    }
    assert!(checked > 100, "only {checked} probes checked");
    assert!(
        failures.is_empty(),
        "Roboto rendering differs from the metrics ({}):\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}
