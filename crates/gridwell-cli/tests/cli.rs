//! End-to-end tests for the `gridwell` binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_gridwell"))
}

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(rel)
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gridwell-cli-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_json(name: &str, json: &serde_json::Value) -> PathBuf {
    let path = scratch_dir(name).join("in.json");
    std::fs::write(&path, json.to_string()).unwrap();
    path
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// A panic exits with 101; every handled failure here must exit with 1.
fn assert_clean_failure(o: &Output) {
    assert_eq!(o.status.code(), Some(1), "stderr: {}", stderr(o));
    assert!(!stderr(o).contains("panicked"), "stderr: {}", stderr(o));
}

const TEXT_FORMATS: &[&str] = &[
    "html", "latex", "typst", "rtf", "svg", "ansi", "pandoc", "quarto",
];
const BINARY_FORMATS: &[&str] = &["docx", "xlsx", "pptx"];

#[test]
fn convert_valid_fixture_to_every_text_format() {
    let input = fixture("comprehensive/reference_table.json");
    for fmt in TEXT_FORMATS {
        let o = bin()
            .args(["convert", "-t", fmt])
            .arg(&input)
            .output()
            .unwrap();
        assert!(o.status.success(), "{fmt}: {}", stderr(&o));
        assert!(!o.stdout.is_empty(), "{fmt}: empty output");
    }
}

#[test]
fn convert_valid_fixture_to_every_binary_format() {
    let input = fixture("comprehensive/reference_table.json");
    let dir = scratch_dir("binary");
    for fmt in BINARY_FORMATS {
        let out = dir.join(format!("out.{fmt}"));
        let o = bin()
            .args(["convert", "-t", fmt])
            .arg(&input)
            .arg("-o")
            .arg(&out)
            .output()
            .unwrap();
        assert!(o.status.success(), "{fmt}: {}", stderr(&o));
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(&bytes[..2], b"PK", "{fmt}: not a zip");
    }
}

#[test]
fn convert_reads_stdin() {
    use std::io::Write;
    use std::process::Stdio;
    let json = std::fs::read(fixture("minimal/minimal_1x1.json")).unwrap();
    let mut child = bin()
        .args(["convert", "-t", "html"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&json).unwrap();
    let o = child.wait_with_output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stdout(&o).contains("<table"));
}

#[test]
fn convert_refuses_invalid_ir_for_every_format() {
    let input = fixture("invalid/span_overflow_right.json");
    for fmt in TEXT_FORMATS {
        let o = bin()
            .args(["convert", "-t", fmt])
            .arg(&input)
            .output()
            .unwrap();
        assert_clean_failure(&o);
        assert!(o.stdout.is_empty(), "{fmt}: wrote output for invalid IR");
        let err = stderr(&o);
        assert!(err.contains("failed validation"), "{fmt}: {err}");
        assert!(err.contains("[SPAN_OVERFLOW_RIGHT]"), "{fmt}: {err}");
    }
}

#[test]
fn convert_refuses_invalid_ir_without_writing_binary_file() {
    let input = fixture("invalid/col_count_mismatch.json");
    let dir = scratch_dir("invalid-binary");
    for fmt in BINARY_FORMATS {
        let out = dir.join(format!("out.{fmt}"));
        let o = bin()
            .args(["convert", "-t", fmt])
            .arg(&input)
            .arg("-o")
            .arg(&out)
            .output()
            .unwrap();
        assert_clean_failure(&o);
        assert!(stderr(&o).contains("[COL_COUNT]"), "{fmt}: {}", stderr(&o));
        assert!(!out.exists(), "{fmt}: output file written for invalid IR");
    }
}

#[test]
fn convert_binary_without_output_fails_before_reading_input() {
    // The input path doesn't exist: the argument error must come first.
    let o = bin()
        .args(["convert", "-t", "docx", "/nonexistent/gridwell.json"])
        .output()
        .unwrap();
    assert_clean_failure(&o);
    assert!(stderr(&o).contains("requires --output"), "{}", stderr(&o));
}

#[test]
fn convert_malformed_json_fails_cleanly() {
    let path = scratch_dir("malformed").join("bad.json");
    std::fs::write(&path, "{ not json").unwrap();
    let o = bin()
        .args(["convert", "-t", "html"])
        .arg(&path)
        .output()
        .unwrap();
    assert_clean_failure(&o);
    assert!(stderr(&o).contains("Error parsing IR"), "{}", stderr(&o));
}

/// Minimal IR builder for crash regressions.
fn one_row_table(cells: Vec<serde_json::Value>) -> serde_json::Value {
    let n = cells.len();
    serde_json::json!({
        "ir_version": "1.0",
        "config": { "table_cols": n, "header_rows": 0, "body_rows": 1 },
        "styles": { "defs": {}, "compositions": {}, "conditionals": [] },
        "column_spec": (0..n).map(|i| serde_json::json!({ "id": format!("c{i}") })).collect::<Vec<_>>(),
        "table": { "thead": { "rows": [] }, "tbody": [{ "rows": [{ "cells": cells }] }] }
    })
}

fn txt(s: &str, colspan: u64) -> serde_json::Value {
    serde_json::json!({ "content": [{ "type": "text", "value": s }], "colspan": colspan })
}

fn ph() -> serde_json::Value {
    serde_json::json!({ "content": [], "is_placeholder": true })
}

#[test]
fn regression_adjacent_colspans_render_in_every_format() {
    // [A cs=2, ph, B cs=2, ph, C, D]: valid IR that used to panic the RTF and SVG
    // writers (usize underflow) and drop cells in DOCX/PPTX/XLSX.
    let path = write_json(
        "adjacent-colspans",
        &one_row_table(vec![
            txt("A", 2),
            ph(),
            txt("B", 2),
            ph(),
            txt("C", 1),
            txt("D", 1),
        ]),
    );
    for fmt in TEXT_FORMATS {
        let o = bin()
            .args(["convert", "-t", fmt])
            .arg(&path)
            .output()
            .unwrap();
        assert!(o.status.success(), "{fmt}: {}", stderr(&o));
        let out = stdout(&o);
        for label in ["A", "B", "C", "D"] {
            assert!(out.contains(label), "{fmt}: missing {label}");
        }
    }
    let dir = scratch_dir("adjacent-colspans-bin");
    for fmt in BINARY_FORMATS {
        let out = dir.join(format!("out.{fmt}"));
        let o = bin()
            .args(["convert", "-t", fmt])
            .arg(&path)
            .arg("-o")
            .arg(&out)
            .output()
            .unwrap();
        assert!(o.status.success(), "{fmt}: {}", stderr(&o));
    }
}

#[test]
fn regression_huge_colspan_is_a_validation_error_not_a_panic() {
    let path = write_json(
        "huge-colspan",
        &one_row_table(vec![txt("a", 1), txt("b", u32::MAX as u64)]),
    );
    for args in [vec!["validate"], vec!["convert", "-t", "html"]] {
        let o = bin().args(&args).arg(&path).output().unwrap();
        assert_clean_failure(&o);
        assert!(
            stderr(&o).contains("[SPAN_OVERFLOW_RIGHT]"),
            "{args:?}: {}",
            stderr(&o)
        );
    }
}

#[test]
fn validate_reports_success_and_failure() {
    let ok = bin()
        .arg("validate")
        .arg(fixture("minimal/minimal_1x1.json"))
        .output()
        .unwrap();
    assert!(ok.status.success(), "{}", stderr(&ok));
    assert!(stderr(&ok).contains("Valid"));

    let bad = bin()
        .arg("validate")
        .arg(fixture("invalid/span_overlap.json"))
        .output()
        .unwrap();
    assert_clean_failure(&bad);
    assert!(stderr(&bad).contains("[SPAN_OVERLAP]"), "{}", stderr(&bad));
}

#[test]
fn formats_lists_every_format() {
    let o = bin().arg("formats").output().unwrap();
    assert!(o.status.success());
    let out = stdout(&o);
    for fmt in TEXT_FORMATS.iter().chain(BINARY_FORMATS) {
        assert!(out.contains(fmt), "missing {fmt}");
    }
}

#[test]
fn formats_json_matches_the_registry() {
    let o = bin().args(["formats", "--json"]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let list: Vec<serde_json::Value> = serde_json::from_slice(&o.stdout).unwrap();
    let names: Vec<&str> = list.iter().map(|f| f["name"].as_str().unwrap()).collect();
    let mut all: Vec<&str> = TEXT_FORMATS.iter().chain(BINARY_FORMATS).copied().collect();
    all.sort();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(sorted, all);
    let html = list.iter().find(|f| f["name"] == "html").unwrap();
    assert_eq!(html["kind"], "text");
    assert_eq!(html["options"]["class_prefix"], "gw");
    let docx = list.iter().find(|f| f["name"] == "docx").unwrap();
    assert_eq!(docx["kind"], "binary");
    assert_eq!(docx["options"], serde_json::json!({}));
    // Quarto output is Pandoc JSON, not a .qmd document.
    let quarto = list.iter().find(|f| f["name"] == "quarto").unwrap();
    assert_eq!(quarto["extension"], "json");
}

#[test]
fn convert_applies_options_inline_and_from_a_file() {
    let input = fixture("comprehensive/reference_table.json");
    let inline = bin()
        .args(["convert", "-t", "latex", "-O", r#"{"booktabs": false}"#])
        .arg(&input)
        .output()
        .unwrap();
    assert!(inline.status.success(), "{}", stderr(&inline));
    assert!(!stdout(&inline).contains("\\toprule") && stdout(&inline).contains("\\hline"));

    let opts = scratch_dir("opts").join("opts.json");
    std::fs::write(&opts, r#"{"booktabs": false}"#).unwrap();
    let from_file = bin()
        .args(["convert", "-t", "latex", "--options-file"])
        .arg(&opts)
        .arg(&input)
        .output()
        .unwrap();
    assert!(from_file.status.success(), "{}", stderr(&from_file));
    assert_eq!(stdout(&inline), stdout(&from_file));
}

#[test]
fn convert_rejects_bad_options_cleanly() {
    let input = fixture("comprehensive/reference_table.json");
    for (opts, needle) in [
        (r#"{"bookabs": false}"#, "unknown field"),
        ("{not json", "not valid JSON"),
        ("[]", "expected a JSON object"),
    ] {
        let o = bin()
            .args(["convert", "-t", "latex", "-O", opts])
            .arg(&input)
            .output()
            .unwrap();
        assert_clean_failure(&o);
        assert!(stderr(&o).contains(needle), "{opts}: {}", stderr(&o));
    }
}

#[test]
fn format_names_are_case_insensitive_and_unknown_ones_list_the_rest() {
    let input = fixture("comprehensive/reference_table.json");
    let o = bin()
        .args(["convert", "-t", "HTML"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let o = bin()
        .args(["convert", "-t", "pdf"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(!o.status.success());
    let err = stderr(&o);
    assert!(err.contains("unknown format \"pdf\""), "{err}");
    for fmt in TEXT_FORMATS.iter().chain(BINARY_FORMATS) {
        assert!(err.contains(fmt), "{err}");
    }
}
