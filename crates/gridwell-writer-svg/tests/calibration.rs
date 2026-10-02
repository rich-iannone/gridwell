//! Calibrate the SVG text-width estimate against real rendering.
//!
//! Each probe string is rendered alone with `rsvg-convert` in the writer's default
//! font stack, and its ink width is measured from the PNG. The estimate must never
//! be narrower than the ink, or laid-out text would overprint neighbouring cells.
//!
//! Fonts differ by machine, which is the point: run locally (e.g. macOS Arial) and in
//! the pinned harness image (Noto / DejaVu). Skipped when `rsvg-convert` is missing
//! unless `GRIDWELL_REQUIRE_RSVG` is set.

use std::process::Command;

use gridwell_writer_svg::measure::text_width;
use gridwell_writer_svg::SvgConfig;

const SIZE: f64 = 50.0;

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

/// Ink width in px of `text` rendered at `SIZE` px, or `None` for blank output.
fn ink_width(text: &str, bold: bool, dir: &std::path::Path, i: usize) -> Option<f64> {
    let family = SvgConfig::default().font_family;
    let weight = if bold { " font-weight=\"bold\"" } else { "" };
    let width = 40.0 + text.chars().count() as f64 * SIZE * 1.6;
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{h}\">\
         <text x=\"20\" y=\"{y}\" font-family=\"{family}\" font-size=\"{SIZE}\"{weight} \
         xml:space=\"preserve\">{t}</text></svg>",
        h = SIZE * 2.0,
        y = SIZE * 1.3,
        t = escape(text),
    );
    let svg_path = dir.join(format!("p{i}.svg"));
    let png_path = dir.join(format!("p{i}.png"));
    std::fs::write(&svg_path, svg).unwrap();
    let out = Command::new("rsvg-convert")
        .args(["--background-color", "white", "-o"])
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
    let (mut min_x, mut max_x) = (u32::MAX, 0u32);
    for (x, _, p) in img.enumerate_pixels() {
        if p.0[..3].iter().any(|&c| c < 200) {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
        }
    }
    (min_x <= max_x).then(|| (max_x - min_x + 1) as f64)
}

#[test]
fn estimate_is_never_narrower_than_rendered_ink() {
    if !require_rsvg() {
        return;
    }
    let mut probes: Vec<String> = (0x21u8..0x7f)
        .map(|b| (b as char).to_string().repeat(8))
        .collect();
    probes.extend(
        [
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
        ]
        .map(String::from),
    );

    let dir = std::env::temp_dir().join(format!("gridwell-svg-cal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let mut failures = Vec::new();
    let mut tightest = (f64::INFINITY, String::new());
    for (i, probe) in probes.iter().enumerate() {
        for bold in [false, true] {
            let Some(ink) = ink_width(probe, bold, &dir, i * 2 + bold as usize) else {
                continue;
            };
            let est = text_width(probe, SIZE, bold);
            let ratio = est / ink;
            if ratio < tightest.0 {
                tightest = (ratio, format!("{probe:?} bold={bold}"));
            }
            // 1px of antialiasing slack.
            if est + 1.0 < ink {
                failures.push(format!(
                    "{probe:?} bold={bold}: estimate {est:.1}px < ink {ink:.1}px"
                ));
            }
        }
    }
    eprintln!(
        "tightest estimate/ink ratio: {:.3} for {}",
        tightest.0, tightest.1
    );
    assert!(
        failures.is_empty(),
        "estimate too narrow:\n  {}",
        failures.join("\n  ")
    );
}
