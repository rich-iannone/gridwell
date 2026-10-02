//! Format-native oracle: every corpus example's SVG must render with `rsvg-convert`
//! (well-formed XML, valid SVG) to a non-blank image.
//!
//! Skipped when `rsvg-convert` is missing unless `GRIDWELL_REQUIRE_RSVG` is set.

use std::process::Command;

use gridwell_testkit::examples;
use gridwell_writer_svg::render_svg;

#[test]
fn every_example_renders_with_rsvg() {
    if Command::new("rsvg-convert")
        .arg("--version")
        .output()
        .is_err()
    {
        if std::env::var_os("GRIDWELL_REQUIRE_RSVG").is_some_and(|v| !v.is_empty()) {
            panic!("rsvg-convert not found but GRIDWELL_REQUIRE_RSVG is set");
        }
        eprintln!("skipping: rsvg-convert not found");
        return;
    }
    let dir = std::env::temp_dir().join(format!("gridwell-svg-rsvg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut failures = Vec::new();
    for ex in examples() {
        let svg = dir.join(format!("{}.svg", ex.name));
        let png = dir.join(format!("{}.png", ex.name));
        std::fs::write(&svg, render_svg(&ex.table()).unwrap()).unwrap();
        let out = Command::new("rsvg-convert")
            .arg("-o")
            .arg(&png)
            .arg(&svg)
            .output()
            .unwrap();
        if !out.status.success() {
            failures.push(format!(
                "{}: {}",
                ex.name,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
            continue;
        }
        let img = image::open(&png).unwrap().to_rgba8();
        if !img.pixels().any(|p| p.0[3] > 0) {
            failures.push(format!("{}: rendered blank", ex.name));
        }
    }
    assert!(
        failures.is_empty(),
        "rsvg failures:\n  {}",
        failures.join("\n  ")
    );
}
