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
const EXPERIMENTAL_FORMATS: &[&str] = &["svg", "ansi", "pandoc", "quarto"];

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

const LATEX_DASHED: &str = "[UNSUPPORTED_FEATURE] latex can't express border-styles (group 1 \
                            row 2, column 1 and 2 other places): a vertical rule is always solid";
const TYPST_DOUBLE: &str = "[UNSUPPORTED_FEATURE] typst can't express border-double (head row \
                            1, column 1 and 2 other places): Typst strokes have no double line";

#[test]
fn validate_warns_of_what_formats_cant_express_and_still_succeeds() {
    let validate = |args: &[&str], rel: &str| {
        bin()
            .arg("validate")
            .args(args)
            .arg(fixture(rel))
            .output()
            .unwrap()
    };
    // Every format, in registry order.
    let o = validate(&[], "minimal/styles_borders.json");
    assert!(o.status.success(), "{}", stderr(&o));
    let err = stderr(&o);
    assert!(
        err.starts_with("Valid: no errors found.\n2 warning(s):\n"),
        "{err}"
    );
    let latex = err.find(LATEX_DASHED).expect(&err);
    assert!(err.find(TYPST_DOUBLE).expect(&err) > latex, "{err}");

    // One format, named as `convert` names it.
    for to in [["--to", "latex"], ["-t", "LaTeX"]] {
        let o = validate(&to, "minimal/styles_borders.json");
        assert!(o.status.success(), "{}", stderr(&o));
        let err = stderr(&o);
        assert!(
            err.contains("1 warning(s):") && err.contains(LATEX_DASHED),
            "{err}"
        );
        assert!(!err.contains("typst"), "{err}");
    }
    let o = validate(&["-t", "html"], "minimal/styles_borders.json");
    assert!(
        o.status.success() && !stderr(&o).contains("warning"),
        "{}",
        stderr(&o)
    );
    let o = validate(&[], "comprehensive/reference_table.json");
    assert!(
        o.status.success() && !stderr(&o).contains("warning"),
        "{}",
        stderr(&o)
    );

    // An unknown format, and an invalid table, fail as they do without.
    let o = validate(&["-t", "pdf"], "minimal/styles_borders.json");
    assert!(!o.status.success());
    assert!(
        stderr(&o).contains("unknown format \"pdf\""),
        "{}",
        stderr(&o)
    );
    let o = validate(&["-t", "latex"], "invalid/span_overlap.json");
    assert_clean_failure(&o);
    assert!(!stderr(&o).contains("warning"), "{}", stderr(&o));
}

#[test]
fn convert_warns_of_what_its_format_cant_express() {
    let input = fixture("minimal/styles_borders.json");
    let o = bin()
        .args(["convert", "-t", "typst"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(!o.stdout.is_empty());
    let err = stderr(&o);
    assert!(
        err.starts_with(&format!("Warning: {TYPST_DOUBLE}")),
        "{err}"
    );
    assert_eq!(err.lines().count(), 1, "{err}");

    // To a file, after saying it's written.
    let out = scratch_dir("warn").join("out.tex");
    let o = bin()
        .arg("convert")
        .arg(&input)
        .arg("-o")
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let err = stderr(&o);
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 2, "{err}");
    assert!(
        lines[0].starts_with("Wrote ") && lines[1].starts_with(&format!("Warning: {LATEX_DASHED}")),
        "{err}"
    );
    assert!(std::fs::metadata(&out).unwrap().len() > 0);

    // A format that expresses it all.
    let o = bin()
        .args(["convert", "-t", "html"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        o.status.success() && stderr(&o).is_empty(),
        "{}",
        stderr(&o)
    );
}

#[test]
fn formats_lists_every_format() {
    let o = bin().arg("formats").output().unwrap();
    assert!(o.status.success());
    let out = stdout(&o);
    for fmt in TEXT_FORMATS.iter().chain(BINARY_FORMATS) {
        let line = out
            .lines()
            .find(|l| l.split_whitespace().next() == Some(fmt))
            .unwrap_or_else(|| panic!("missing {fmt}"));
        let experimental = EXPERIMENTAL_FORMATS.contains(fmt);
        assert_eq!(line.ends_with(", experimental)"), experimental, "{line}");
    }
    assert!(
        out.contains("Experimental formats write valid output with every cell"),
        "{out}"
    );
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
    for f in &list {
        let experimental = EXPERIMENTAL_FORMATS.contains(&f["name"].as_str().unwrap());
        assert_eq!(f["experimental"], experimental, "{}", f["name"]);
    }
    let html = list.iter().find(|f| f["name"] == "html").unwrap();
    assert_eq!(html["kind"], "text");
    assert_eq!(html["options"]["class_prefix"], "gw");
    let docx = list.iter().find(|f| f["name"] == "docx").unwrap();
    assert_eq!(docx["kind"], "binary");
    assert_eq!(docx["options"], serde_json::json!({ "raw": true }));
    assert!(list.iter().all(|f| f["options"]["raw"] == true), "{list:?}");
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
fn convert_with_raw_off_writes_no_raw_markup() {
    let path = write_json(
        "raw-off",
        &one_row_table(vec![serde_json::json!({ "content": [
            { "type": "raw", "format": "html", "value": "<b>hi</b><script>alert(1)</script>" },
            { "type": "raw", "format": "latex", "value": "\\input{/etc/passwd}" },
        ] })]),
    );
    let convert = |to: &str, opts: Option<&str>| {
        let mut cmd = bin();
        cmd.args(["convert", "-t", to]);
        if let Some(opts) = opts {
            cmd.args(["-O", opts]);
        }
        let o = cmd.arg(&path).output().unwrap();
        assert!(o.status.success(), "{to}: {}", stderr(&o));
        stdout(&o)
    };
    assert!(convert("html", None).contains("<script>"));
    assert!(convert("latex", None).contains("\\input"));
    let html = convert("html", Some(r#"{"raw": false}"#));
    assert!(
        !html.contains("<script>") && html.contains(">hi</span>"),
        "{html}"
    );
    let latex = convert("latex", Some(r#"{"raw": false, "booktabs": false}"#));
    assert!(
        !latex.contains("\\input") && latex.contains("\\textbf{hi}"),
        "{latex}"
    );
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

#[test]
fn convert_infers_the_format_from_the_output_extension() {
    let input = fixture("comprehensive/reference_table.json");
    let dir = scratch_dir("infer");
    for (file, fmt) in [
        ("out.html", "html"),
        ("out.TEX", "latex"),
        ("out.typ", "typst"),
        ("out.rtf", "rtf"),
        ("out.svg", "svg"),
        ("out.docx", "docx"),
        ("out.xlsx", "xlsx"),
        ("out.pptx", "pptx"),
    ] {
        let out = dir.join(file);
        let o = bin()
            .arg("convert")
            .arg(&input)
            .arg("-o")
            .arg(&out)
            .output()
            .unwrap();
        assert!(o.status.success(), "{file}: {}", stderr(&o));
        assert!(
            stderr(&o).contains(&format!("{fmt})")),
            "{file}: {}",
            stderr(&o)
        );
        // Same bytes as an explicit --to to stdout (text) or file (binary).
        let explicit = dir.join(format!("explicit-{file}"));
        let e = bin()
            .args(["convert", "-t", fmt])
            .arg(&input)
            .arg("-o")
            .arg(&explicit)
            .output()
            .unwrap();
        assert!(e.status.success(), "{fmt}: {}", stderr(&e));
        assert_eq!(
            std::fs::read(&out).unwrap(),
            std::fs::read(&explicit).unwrap()
        );
        if !BINARY_FORMATS.contains(&fmt) {
            let s = bin()
                .args(["convert", "-t", fmt])
                .arg(&input)
                .output()
                .unwrap();
            assert_eq!(std::fs::read(&out).unwrap(), s.stdout, "{fmt}");
        }
    }
}

#[test]
fn convert_to_a_txt_file_is_plain_text_unless_color_is_set() {
    let input = fixture("comprehensive/reference_table.json");
    let dir = scratch_dir("txt");
    let out = dir.join("out.txt");
    let o = bin()
        .arg("convert")
        .arg(&input)
        .arg("-o")
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let plain = std::fs::read_to_string(&out).unwrap();
    assert!(!plain.is_empty() && !plain.contains('\x1b'));

    // stdout keeps the terminal default (colored).
    let s = bin()
        .args(["convert", "-t", "ansi"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(stdout(&s).contains('\x1b'));

    let o = bin()
        .arg("convert")
        .arg(&input)
        .args(["-O", r#"{"color": true}"#, "-o"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(std::fs::read(&out).unwrap(), s.stdout);
}

#[test]
fn convert_explicit_to_overrides_the_output_extension() {
    let input = fixture("comprehensive/reference_table.json");
    let dir = scratch_dir("override");
    for fmt in ["pandoc", "quarto"] {
        let out = dir.join(format!("{fmt}.json"));
        let o = bin()
            .args(["convert", "-t", fmt])
            .arg(&input)
            .arg("-o")
            .arg(&out)
            .output()
            .unwrap();
        assert!(o.status.success(), "{fmt}: {}", stderr(&o));
        let s = bin()
            .args(["convert", "-t", fmt])
            .arg(&input)
            .output()
            .unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), s.stdout, "{fmt}");
    }
}

#[test]
fn convert_extension_errors_fail_before_reading_input_and_write_nothing() {
    let dir = scratch_dir("ext-errors");
    for (file, needle) in [
        ("out.json", "pandoc or quarto"),
        ("out.pdf", "no format writes \".pdf\" files"),
        ("out", "no file extension"),
    ] {
        let out = dir.join(file);
        let o = bin()
            .args(["convert", "/nonexistent/gridwell.json", "-o"])
            .arg(&out)
            .output()
            .unwrap();
        assert_clean_failure(&o);
        let err = stderr(&o);
        assert!(err.contains(needle), "{file}: {err}");
        assert!(err.contains("with --to"), "{file}: {err}");
        assert!(!out.exists(), "{file} written");
    }
}

#[test]
fn convert_needs_to_or_output() {
    let input = fixture("comprehensive/reference_table.json");
    let o = bin().arg("convert").arg(&input).output().unwrap();
    assert!(!o.status.success());
    assert!(o.stdout.is_empty());
    assert!(stderr(&o).contains("--to"), "{}", stderr(&o));
}

// ─── conformance ───

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn json_out(o: &Output) -> serde_json::Value {
    serde_json::from_slice(&o.stdout).unwrap_or_else(|e| panic!("{e}: {}", stdout(o)))
}

#[test]
fn conformance_normalize_writes_the_expected_grid() {
    let o = bin()
        .args(["conformance", "normalize"])
        .arg(repo("harness/gt-reference/demographics.ir.json"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let grid = json_out(&o);
    let expected: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo("conformance/expected/gt/demographics.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(grid["format"], "gridwell-conformance-grid/1");
    assert_eq!(grid["tier"], "L3");
    assert_eq!(grid["grid"], expected["grid"]);
}

#[test]
fn conformance_normalize_to_a_tier_and_from_stdin() {
    let ir = std::fs::read_to_string(fixture("minimal/minimal_1x1.json")).unwrap();
    let mut child = bin()
        .args(["conformance", "normalize", "--tier", "l1"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(ir.as_bytes())
        .unwrap();
    let o = child.wait_with_output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let grid = json_out(&o);
    assert_eq!(grid["tier"], "L1");
    assert_eq!(
        grid["grid"]["groups"][0]["rows"][0]["cells"],
        serde_json::json!([{"col": 0, "text": "Hello"}])
    );
    assert!(!stdout(&o).contains("\"runs\""));
}

#[test]
fn conformance_normalize_refuses_what_it_cant_normalize() {
    let o = bin()
        .args(["conformance", "normalize"])
        .arg(fixture("invalid/col_count_mismatch.json"))
        .output()
        .unwrap();
    assert_clean_failure(&o);
    assert!(stderr(&o).contains("validation"), "{}", stderr(&o));
    let path = scratch_dir("normalize-malformed").join("in.json");
    std::fs::write(&path, "{not json").unwrap();
    let o = bin()
        .args(["conformance", "normalize"])
        .arg(&path)
        .output()
        .unwrap();
    assert_clean_failure(&o);
    assert!(stderr(&o).contains("Error parsing IR"), "{}", stderr(&o));
    let o = bin()
        .args(["conformance", "normalize", "--tier", "L4"])
        .arg(fixture("minimal/minimal_1x1.json"))
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    assert!(stderr(&o).contains("unknown tier \"L4\""), "{}", stderr(&o));
}

#[test]
fn conformance_check_passes_and_fails() {
    let o = bin()
        .args(["conformance", "check"])
        .arg(repo("harness/gt-reference/tab_style.ir.json"))
        .arg(repo("conformance/expected/gt/tab_style.json"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}{}", stdout(&o), stderr(&o));
    assert!(stdout(&o).starts_with("passes L3\n"), "{}", stdout(&o));
    // Another case's IR.
    let o = bin()
        .args(["conformance", "check"])
        .arg(repo("harness/gt-reference/exibble_basic.ir.json"))
        .arg(repo("conformance/expected/gt/tab_style.json"))
        .output()
        .unwrap();
    assert_clean_failure(&o);
    assert!(
        stdout(&o).starts_with("fails L1 (checked to L3)\n"),
        "{}",
        stdout(&o)
    );
    assert!(stdout(&o).contains(" differences:\n  L"), "{}", stdout(&o));
    // To a lower tier, and as JSON.
    let o = bin()
        .args(["conformance", "check", "--json", "--tier", "L0"])
        .arg(repo("harness/gt-reference/exibble_basic.ir.json"))
        .arg(repo("conformance/expected/gt/tab_style.json"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let report = json_out(&o);
    assert_eq!(report["passed"], true);
    assert_eq!(report["tiers"]["L1"], "not checked");
    assert_eq!(report["differences"], serde_json::json!([]));
}

#[test]
fn conformance_check_of_ir_that_isnt_valid_fails_l0() {
    let path = scratch_dir("check-malformed").join("in.json");
    std::fs::write(&path, "{not json").unwrap();
    for ir in [path, fixture("invalid/col_count_mismatch.json")] {
        let o = bin()
            .args(["conformance", "check", "--json"])
            .arg(&ir)
            .arg(repo("conformance/expected/gt/tab_style.json"))
            .output()
            .unwrap();
        assert_clean_failure(&o);
        let report = json_out(&o);
        assert_eq!(report["tiers"]["L0"], "fail", "{}", ir.display());
        assert!(!report["errors"].as_array().unwrap().is_empty());
    }
}

#[test]
fn conformance_check_refuses_what_it_cant_check() {
    let tab_style = repo("harness/gt-reference/tab_style.ir.json");
    // A tier the expected grid doesn't go to.
    let o = bin()
        .args(["conformance", "normalize", "--tier", "L1"])
        .arg(&tab_style)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let l1 = scratch_dir("check-tier").join("l1.json");
    std::fs::write(&l1, &o.stdout).unwrap();
    let o = bin()
        .args(["conformance", "check", "--tier", "L2"])
        .arg(&tab_style)
        .arg(&l1)
        .output()
        .unwrap();
    assert_clean_failure(&o);
    assert!(
        stderr(&o).contains("goes to L1, so it can't be checked to L2"),
        "{}",
        stderr(&o)
    );
    // Not a grid: an IR file.
    let o = bin()
        .args(["conformance", "check"])
        .arg(&tab_style)
        .arg(&tab_style)
        .output()
        .unwrap();
    assert_clean_failure(&o);
    assert!(
        stderr(&o).contains("the expected grid has no format"),
        "{}",
        stderr(&o)
    );
    // No such file.
    let o = bin()
        .args(["conformance", "check"])
        .arg(&tab_style)
        .arg(scratch_dir("check-missing").join("nope.json"))
        .output()
        .unwrap();
    assert_clean_failure(&o);
    assert!(stderr(&o).contains("reading "), "{}", stderr(&o));
}

fn versioned(name: &str, version: &str) -> PathBuf {
    let mut ir = one_row_table(vec![txt("a", 1)]);
    ir["ir_version"] = serde_json::json!(version);
    write_json(name, &ir)
}

const NEWER: &str = "[NEWER_IR_VERSION] the table is IR 1.3, newer than the IR 1.0 this \
                     gridwell reads";

#[test]
fn a_newer_minor_version_converts_and_validates_with_a_warning() {
    let path = versioned("newer-minor", "1.3");
    let o = bin()
        .args(["convert", "-t", "html"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stdout(&o).contains("<table"));
    let err = stderr(&o);
    assert!(err.starts_with(&format!("Warning: {NEWER}")), "{err}");
    assert_eq!(err.lines().count(), 1, "{err}");

    let o = bin().arg("validate").arg(&path).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let err = stderr(&o);
    assert!(
        err.starts_with(&format!(
            "Valid: no errors found.\n1 warning(s):\n  - {NEWER}"
        )),
        "{err}"
    );
}

#[test]
fn an_invalid_newer_minor_version_says_its_version() {
    let mut ir = one_row_table(vec![txt("a", 1)]);
    ir["ir_version"] = serde_json::json!("1.3");
    ir["config"]["table_cols"] = serde_json::json!(2);
    let path = write_json("newer-minor-invalid", &ir);
    let note = "(the table is IR 1.3, newer than the IR 1.0 this gridwell reads: an error \
                may be something a newer IR version added)";
    for args in [vec!["validate"], vec!["convert", "-t", "html"]] {
        let o = bin().args(&args).arg(&path).output().unwrap();
        assert_clean_failure(&o);
        let err = stderr(&o);
        assert!(err.contains("[COL_COUNT]"), "{args:?}: {err}");
        assert!(err.trim_end().ends_with(note), "{args:?}: {err}");
    }
    // Not for this version.
    let o = bin()
        .arg("validate")
        .arg(fixture("invalid/span_overlap.json"))
        .output()
        .unwrap();
    assert!(!stderr(&o).contains("newer"), "{}", stderr(&o));
}

#[test]
fn another_major_version_is_refused() {
    // It parses as IR 1: a validation error.
    let path = versioned("unknown-major", "2.0");
    for args in [vec!["validate"], vec!["convert", "-t", "html"]] {
        let o = bin().args(&args).arg(&path).output().unwrap();
        assert_clean_failure(&o);
        assert!(
            stderr(&o).contains(
                "[IR_VERSION] ir_version is \"2.0\", IR 2, and this gridwell reads only IR \
                 1.x (a newer gridwell may read it)"
            ),
            "{args:?}: {}",
            stderr(&o)
        );
        assert!(o.stdout.is_empty());
    }
    // It doesn't: the version is what's wrong, not the shape.
    let mut ir = one_row_table(vec![txt("a", 1)]);
    ir["ir_version"] = serde_json::json!("2.0");
    ir["table"] = serde_json::json!({ "sections": [] });
    let path = write_json("unknown-major-shape", &ir);
    let o = bin().arg("validate").arg(&path).output().unwrap();
    assert_clean_failure(&o);
    assert_eq!(
        stderr(&o),
        "Error parsing IR: ir_version is \"2.0\", IR 2, and this gridwell reads only IR 1.x \
         (a newer gridwell may read it)\n"
    );
}

// ─── fmt, inspect, grid ───

fn with_stdin(args: &[&str], input: &[u8]) -> Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = bin()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

/// Every IR file in the repository that parses: the fixtures and the docs' examples.
fn ir_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in [
        "fixtures/minimal",
        "fixtures/comprehensive",
        "fixtures/invalid",
        "docs/examples",
    ] {
        for entry in std::fs::read_dir(repo(dir)).unwrap() {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            if path.extension().is_some_and(|e| e == "json") && text.contains("\"ir_version\"") {
                files.push(path);
            }
        }
    }
    files.sort();
    assert!(files.len() > 30, "{files:?}");
    files
}

#[test]
fn fmt_sorts_keys_at_every_depth_and_keeps_every_field() {
    let ir = r#"{"table":{"tbody":[{"rows":[{"cells":[{"content":[{"value":"x","type":"text"},
        {"type":"sparkline","values":[1,2.5]}],"zz_future":true}]}]}],"thead":{"rows":[]}},
        "styles":{},"ir_version":"1.0","config":{"table_cols":1,"header_rows":0,"body_rows":1},
        "column_spec":[{"id":"a"}]}"#;
    let o = with_stdin(&["fmt"], ir.as_bytes());
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(
        stdout(&o),
        r#"{
  "column_spec": [
    {
      "id": "a"
    }
  ],
  "config": {
    "body_rows": 1,
    "header_rows": 0,
    "table_cols": 1
  },
  "ir_version": "1.0",
  "styles": {},
  "table": {
    "tbody": [
      {
        "rows": [
          {
            "cells": [
              {
                "content": [
                  {
                    "type": "text",
                    "value": "x"
                  },
                  {
                    "type": "sparkline",
                    "values": [
                      1,
                      2.5
                    ]
                  }
                ],
                "zz_future": true
              }
            ]
          }
        ]
      }
    ],
    "thead": {
      "rows": []
    }
  }
}
"#
    );
    assert!(stderr(&o).is_empty(), "{}", stderr(&o));
}

#[test]
fn fmt_is_lossless_idempotent_and_agrees_with_check() {
    for path in ir_files() {
        let name = path.display().to_string();
        let original = std::fs::read_to_string(&path).unwrap();
        let o = bin().arg("fmt").arg(&path).output().unwrap();
        assert!(o.status.success(), "{name}: {}", stderr(&o));
        let formatted = stdout(&o);
        // The same JSON: nothing added, dropped or changed.
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&formatted).unwrap(),
            serde_json::from_str::<serde_json::Value>(&original).unwrap(),
            "{name}"
        );
        assert!(
            formatted.ends_with("}\n") && !formatted.contains('\r'),
            "{name}"
        );
        // Formatting what's formatted changes nothing, and --check says so.
        let again = with_stdin(&["fmt"], formatted.as_bytes());
        assert_eq!(stdout(&again), formatted, "{name}");
        let check = with_stdin(&["fmt", "--check"], formatted.as_bytes());
        assert!(check.status.success(), "{name}: {}", stderr(&check));
        assert!(check.stdout.is_empty() && check.stderr.is_empty(), "{name}");
        // None of the files is written that way, so --check fails on each.
        let check = bin().args(["fmt", "--check"]).arg(&path).output().unwrap();
        assert_clean_failure(&check);
        assert!(check.stdout.is_empty(), "{name}");
        assert_eq!(
            stderr(&check),
            format!("{name} is not in canonical form (see `gridwell fmt`)\n")
        );
    }
}

#[test]
fn fmt_check_is_byte_exact() {
    let canonical = stdout(
        &bin()
            .arg("fmt")
            .arg(fixture("minimal/minimal_1x1.json"))
            .output()
            .unwrap(),
    );
    for (what, text) in [
        ("CRLF line endings", canonical.replace('\n', "\r\n")),
        ("no final newline", canonical.trim_end().to_string()),
        ("a trailing blank line", format!("{canonical}\n")),
        ("four-space indents", canonical.replace("\n  ", "\n    ")),
        ("a leading space", format!(" {canonical}")),
    ] {
        let o = with_stdin(&["fmt", "--check"], text.as_bytes());
        assert_clean_failure(&o);
        assert_eq!(
            stderr(&o),
            "stdin is not in canonical form (see `gridwell fmt`)\n",
            "{what}"
        );
        // And formatting it gives the canonical form back.
        assert_eq!(
            stdout(&with_stdin(&["fmt"], text.as_bytes())),
            canonical,
            "{what}"
        );
    }
    let o = with_stdin(&["fmt", "--check", "-"], canonical.as_bytes());
    assert!(o.status.success(), "{}", stderr(&o));
}

#[test]
fn fmt_needs_ir_but_not_valid_ir() {
    // Invalid IR formats: the formatter is for fixing it too.
    let o = bin()
        .arg("fmt")
        .arg(fixture("invalid/span_overlap.json"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    // JSON that isn't IR, and text that isn't JSON, don't.
    for (input, error) in [
        (
            r#"{"a": 1}"#,
            "Error parsing IR: JSON parse error: missing field `ir_version`",
        ),
        (
            "{ not json",
            "Error parsing IR: JSON parse error: key must be a string",
        ),
        ("", "Error parsing IR: JSON parse error: EOF while parsing"),
    ] {
        for args in [vec!["fmt"], vec!["fmt", "--check"]] {
            let o = with_stdin(&args, input.as_bytes());
            assert_clean_failure(&o);
            assert!(o.stdout.is_empty(), "{input}");
            assert!(stderr(&o).starts_with(error), "{input}: {}", stderr(&o));
        }
    }
    let o = bin()
        .args(["fmt", "/nonexistent/gridwell.json"])
        .output()
        .unwrap();
    assert_clean_failure(&o);
    assert!(
        stderr(&o).starts_with("Error reading /nonexistent/gridwell.json"),
        "{}",
        stderr(&o)
    );
}

#[test]
fn inspect_counts_what_the_table_holds() {
    let o = bin()
        .arg("inspect")
        .arg(fixture("comprehensive/reference_table.json"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(
        stdout(&o),
        "\
IR version:    1.0
Valid:         yes
Columns:       5 (1 stub)
Header:        title, subtitle
Head rows:     2
Body:          6 rows in 2 groups (2 labeled)
Summary rows:  2
Spans:         3 spanning cells, 4 placeholders
Styles:        14 definitions, 0 compositions, 0 conditionals; 13 used
Unused styles: footnote_mark
Footnotes:     1
Source notes:  1
Content:       1 footnote_mark, 50 text
Typed values:  6 integer, 24 number, 4 string
Extensions:    html, latex
"
    );
    let o = bin()
        .args(["inspect", "--json"])
        .arg(fixture("comprehensive/reference_table.json"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let stats = json_out(&o);
    for (field, value) in [
        ("ir_version", serde_json::json!("1.0")),
        ("valid", serde_json::json!(true)),
        ("errors", serde_json::json!(0)),
        (
            "columns",
            serde_json::json!({ "total": 5, "stub": 1, "hidden": [] }),
        ),
        (
            "header",
            serde_json::json!({ "title": true, "subtitle": true, "extra_lines": 0 }),
        ),
        ("head_rows", serde_json::json!(2)),
        ("groups", serde_json::json!(2)),
        ("labeled_groups", serde_json::json!(2)),
        ("body_rows", serde_json::json!(6)),
        ("summary_rows", serde_json::json!(2)),
        ("grand_summary_rows", serde_json::json!(0)),
        ("spanning_cells", serde_json::json!(3)),
        ("placeholders", serde_json::json!(4)),
        ("footnotes", serde_json::json!(1)),
        ("source_notes", serde_json::json!(1)),
        (
            "content",
            serde_json::json!({ "footnote_mark": 1, "text": 50 }),
        ),
        (
            "typed_values",
            serde_json::json!({ "integer": 6, "number": 24, "string": 4 }),
        ),
        ("extensions", serde_json::json!(["html", "latex"])),
    ] {
        assert_eq!(stats[field], value, "{field}");
    }
    assert_eq!(stats["styles"]["definitions"], 14);
    assert_eq!(stats["styles"]["used"].as_array().unwrap().len(), 13);
    assert_eq!(
        stats["styles"]["unused"],
        serde_json::json!(["footnote_mark"])
    );
    assert_eq!(stats.as_object().unwrap().len(), 19, "{stats}");
}

/// A table with one of everything `inspect` counts.
fn everything_table() -> serde_json::Value {
    let mut ir = one_row_table(vec![
        serde_json::json!({ "content": [{ "type": "styled_text", "value": "a", "style_id": "a" }],
            "style_id": "comp", "is_stub": true }),
        serde_json::json!({ "content": [{ "type": "sparkline", "values": [1] }],
            "typed_value": { "type": "date", "value": "2026-01-01" } }),
        serde_json::json!({ "content": [{ "type": "image", "src": "data:," }, { "type": "line_break" }] }),
    ]);
    ir["config"]["stub_cols"] = serde_json::json!(1);
    ir["column_spec"][1]["hidden"] = serde_json::json!(true);
    ir["column_spec"][2]["hidden"] = serde_json::json!(true);
    ir["column_spec"][2]["id"] = serde_json::json!("x\u{1b}[31m");
    // A definition for each place a style can be used, and one used by nothing.
    let defs: serde_json::Map<String, serde_json::Value> = "abcdefghij"
        .chars()
        .map(|c| (c.to_string(), serde_json::json!({})))
        .collect();
    ir["styles"] = serde_json::json!({
        "defs": defs,
        "compositions": {
            "comp": { "extends": "b", "overrides": {} },
            "spare": { "extends": "c", "overrides": {} }
        },
        "conditionals": [{ "id": "z", "selector": { "row_parity": "odd" }, "style": {} }]
    });
    ir["column_spec"][0]["style_id"] = serde_json::json!("f");
    ir["header"] = serde_json::json!({
        "subtitle": { "content": [{ "type": "text", "value": "s" }], "style_id": "j" },
        "extra_lines": [{ "content": [] }, { "content": [], "style_id": "e" }]
    });
    let row = ir["table"]["tbody"][0]["rows"][0].clone();
    let group = &mut ir["table"]["tbody"][0];
    group["label"] =
        serde_json::json!({ "content": [{ "type": "text", "value": "G" }], "style_id": "g" });
    group["summary_rows"] = serde_json::json!([row.clone(), row]);
    group["summary_rows"][1]["role"] = serde_json::json!("grand_summary_row");
    group["summary_rows"][1]["style_id"] = serde_json::json!("d");
    ir["table"]["tbody"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({ "rows": [] }));
    ir["footer"] = serde_json::json!({
        "footnotes": [{ "id": "n", "mark": "1", "content": [{ "type": "text", "value": "n" }],
            "style_id": "h" }],
        "source_notes": [{ "content": [{ "type": "text", "value": "src" }], "style_id": "i" }]
    });
    ir["extensions"] = serde_json::json!({ "zeta": 1, "alpha": {} });
    ir
}

#[test]
fn inspect_counts_one_of_everything() {
    let path = write_json("inspect-everything", &everything_table());
    let o = bin().arg("inspect").arg(&path).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let text = stdout(&o);
    // The ids are the table's; an escape sequence in one is shown escaped.
    assert!(!text.contains('\u{1b}'), "{text}");
    assert_eq!(
        text,
        "\
IR version:    1.0
Valid:         yes
Columns:       3 (1 stub, 2 hidden: c1, \"x\\u{1b}[31m\")
Header:        subtitle, 2 extra lines
Head rows:     0
Body:          1 row in 2 groups (1 labeled)
Summary rows:  2 (1 grand summary)
Spans:         none
Styles:        10 definitions, 2 compositions, 1 conditional; 9 used
Unused styles: c, spare
Footnotes:     1
Source notes:  1
Content:       3 image, 3 line_break, 3 sparkline, 3 styled_text, 4 text
Typed values:  3 date
Extensions:    alpha, zeta
"
    );
    let o = bin()
        .args(["inspect", "--json"])
        .arg(&path)
        .output()
        .unwrap();
    let stats = json_out(&o);
    assert_eq!(stats["valid"], true);
    assert_eq!(stats["summary_rows"], 2);
    assert_eq!(stats["grand_summary_rows"], 1);
    assert_eq!(
        stats["styles"]["used"],
        serde_json::json!(["a", "comp", "d", "e", "f", "g", "h", "i", "j"])
    );
    assert_eq!(stats["styles"]["unused"], serde_json::json!(["c", "spare"]));
    assert_eq!(
        stats["columns"]["hidden"],
        serde_json::json!(["c1", "x\u{1b}[31m"])
    );
}

#[test]
fn inspect_counts_spans_of_any_size_and_placeholders_apart() {
    let ir = one_row_table(vec![
        txt("a", 2),
        serde_json::json!({ "content": [], "is_placeholder": true, "colspan": 3 }),
        txt("b", 0),
        serde_json::json!({ "content": [{ "type": "text", "value": "c" }], "rowspan": 2 }),
        txt("d", 1),
    ]);
    let o = bin()
        .args(["inspect", "--json"])
        .arg(write_json("inspect-spans", &ir))
        .output()
        .unwrap();
    let stats = json_out(&o);
    assert_eq!(stats["spanning_cells"], 3, "{stats}");
    assert_eq!(stats["placeholders"], 1, "{stats}");
    assert_eq!(stats["valid"], false, "{stats}");
}

#[test]
fn inspect_and_grid_read_invalid_ir_and_stdin() {
    let broken = std::fs::read(repo("docs/examples/broken_table.json")).unwrap();
    let o = with_stdin(&["inspect"], &broken);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stdout(&o).contains("\nValid:         no, 2 errors (see `gridwell validate`)\n"));
    let o = with_stdin(&["grid", "-"], &broken);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        stdout(&o).contains("[SPAN_OVERFLOW_RIGHT]"),
        "{}",
        stdout(&o)
    );
    for args in [vec!["inspect"], vec!["inspect", "--json"], vec!["grid"]] {
        let o = with_stdin(&args, b"{ not json");
        assert_clean_failure(&o);
        assert!(o.stdout.is_empty());
        assert!(stderr(&o).starts_with("Error parsing IR"), "{}", stderr(&o));
    }
}

#[test]
fn grid_draws_each_section() {
    let o = bin()
        .arg("grid")
        .arg(fixture("comprehensive/reference_table.json"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(
        stdout(&o),
        r#"Stub columns: c0

thead: 2 rows, 5 columns
    c0  c1  c2  c3  c4
r0 [A ][B ][C ][C ][C ]
r1 [D ][E ][F ][G ][H ]
C = cell(r0,c2, colspan=3) "Financials"

tbody group 0 "North America": 3 rows, 5 columns
    c0  c1  c2  c3  c4
r0 [A ][B ][C ][D ][E ]
r1 [A ][F ][G ][H ][I ]
r2 [J ][K ][L ][M ][N ]
A = cell(r0,c0, rowspan=2) "United States"

tbody group 0 summary: 1 row, 5 columns
    c0  c1  c2  c3  c4
r0 [A ][B ][C ][D ][E ]

tbody group 1 "Europe": 3 rows, 5 columns
    c0  c1  c2  c3  c4
r0 [A ][B ][C ][D ][E ]
r1 [A ][F ][G ][H ][I ]
r2 [J ][K ][L ][M ][N ]
A = cell(r0,c0, rowspan=2) "Germany"

tbody group 1 summary: 1 row, 5 columns
    c0  c1  c2  c3  c4
r0 [A ][B ][C ][D ][E ]
"#
    );
}

#[test]
fn grid_draws_summary_rows_at_the_top_first() {
    let path = fixture("comprehensive/reference_table.json");
    let mut ir: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    ir["table"]["tbody"][0]["summary_placement"] = serde_json::json!("top");
    let o = with_stdin(&["grid"], ir.to_string().as_bytes());
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    let at = |s: &str| out.find(s).unwrap_or_else(|| panic!("{s}: {out}"));
    assert!(at("tbody group 0 summary (top): 1 row") < at("tbody group 0 \"North America\""));
    assert!(at("tbody group 1 \"Europe\"") < at("tbody group 1 summary: 1 row"));
}

#[test]
fn grid_draws_span_errors_where_they_are() {
    let grid = |rel: &str| {
        let o = bin().arg("grid").arg(fixture(rel)).output().unwrap();
        // A view, not a check: it succeeds for any IR that parses.
        assert!(o.status.success(), "{rel}: {}", stderr(&o));
        stdout(&o)
    };
    let overlap = grid("invalid/span_overlap.json");
    assert!(
        overlap.contains(
            "\
r0 [A ][A ][B ]
r1 [A ][A ][D ]
A = cell(r0,c0, rowspan=2, colspan=2) \"spans right\"
C = cell(r1,c0, colspan=2) \"CONFLICT\"
! [SPAN_OVERLAP] Grid position at tbody group 0 (row=1, col=0) is already claimed by the cell at (row=0, col=0)
"
        ),
        "{overlap}"
    );
    // A position nothing covers.
    let mismatch = grid("invalid/span_placeholder_mismatch.json");
    assert!(
        mismatch.contains("r0 [A ][. ]\n! [SPAN_PLACEHOLDER_MISMATCH]"),
        "{mismatch}"
    );
    let zero = grid("invalid/span_zero_value.json");
    assert!(
        zero.contains("A = cell(r0,c0, colspan=0) \"1\"\n! [SPAN_ZERO_VALUE]"),
        "{zero}"
    );
    let bottom = grid("invalid/span_overflow_bottom.json");
    assert!(
        bottom.contains("r1 [A ][C ]\nA = cell(r0,c0, rowspan=3)"),
        "{bottom}"
    );
    let count = grid("invalid/col_count_mismatch.json");
    assert!(
        count.contains("tbody group 0: not drawn, row 0 has 4 cells but config.table_cols is 3\n"),
        "{count}"
    );
}

#[test]
fn grid_lists_every_error_and_excerpts_every_kind_of_text() {
    // Two zero spans in one section: both errors are listed. The excerpt has the
    // text of each kind of node that has some.
    let mut ir = one_row_table(vec![txt("a", 0), txt("b", 0), txt("c", 1)]);
    ir["table"]["tbody"][0]["rows"][0]["cells"][0]["content"] = serde_json::json!([
        { "type": "styled_text", "value": "Bold", "style_id": "s" },
        { "type": "line_break" },
        { "type": "text", "value": "x" },
        { "type": "footnote_mark", "ref": "f", "mark_text": "1" },
        { "type": "image", "src": "data:," },
        { "type": "image", "src": "data:,", "alt": "logo" },
        { "type": "raw", "format": "html", "value": "<b>" },
    ]);
    let o = bin()
        .arg("grid")
        .arg(write_json("grid-errors", &ir))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    let lines: Vec<&str> = out.lines().skip_while(|l| !l.starts_with("A = ")).collect();
    assert_eq!(
        lines[0],
        r#"A = cell(r0,c0, colspan=0) "Bold x1[image]logo""#
    );
    assert_eq!(lines[1], r#"B = cell(r0,c1, colspan=0) "b""#);
    let errors: Vec<&&str> = lines.iter().filter(|l| l.starts_with("! ")).collect();
    assert_eq!(errors.len(), 2, "{out}");
    assert!(
        errors.iter().all(|l| l.contains("[SPAN_ZERO_VALUE]")),
        "{out}"
    );
}

#[test]
fn grid_codes_go_past_z() {
    let cells = (0..30).map(|i| txt(&i.to_string(), 1)).collect();
    let mut ir = one_row_table(cells);
    ir["table"]["thead"]["rows"] = serde_json::json!([]);
    let o = bin()
        .arg("grid")
        .arg(write_json("grid-codes", &ir))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "thead: no rows");
    assert_eq!(lines[2], "tbody group 0: 1 row, 30 columns");
    // Three wide, for c29.
    assert!(lines[3].starts_with("    c0   c1   c2 "), "{}", lines[3]);
    assert!(lines[3].ends_with(" c28  c29"), "{}", lines[3]);
    assert!(lines[4].starts_with("r0 [A  ][B  ]"), "{}", lines[4]);
    assert!(
        lines[4].ends_with("[Y  ][Z  ][AA ][AB ][AC ][AD ]"),
        "{}",
        lines[4]
    );
    // Each code is under its column's name.
    for c in [0, 9, 10, 29] {
        let label = lines[3]
            .find(&format!(" c{c} ").trim_end().to_string())
            .unwrap();
        assert_eq!(&lines[4][label..label + 1], "[", "c{c}");
    }
    assert_eq!(lines.len(), 5, "{out}");
}

#[test]
fn grid_of_hostile_ir_is_bounded_and_escaped() {
    // table_cols far beyond the cells present: not drawn, and nothing that size
    // allocated.
    let mut ir = one_row_table(vec![txt("a", 1)]);
    ir["config"]["table_cols"] = serde_json::json!(u32::MAX);
    let o = bin()
        .arg("grid")
        .arg(write_json("grid-wide", &ir))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stdout(&o).contains("not drawn, row 0 has 1 cells but config.table_cols is 4294967295"));
    // A span as big as can be is clamped.
    let ir = one_row_table(vec![txt("a", 1), txt("b", u32::MAX as u64)]);
    let o = bin()
        .arg("grid")
        .arg(write_json("grid-huge-span", &ir))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        stdout(&o).contains(
            "r0 [A ][B ]\nB = cell(r0,c1, colspan=4294967295) \"b\"\n! [SPAN_OVERFLOW_RIGHT]"
        ),
        "{}",
        stdout(&o)
    );
    // Text from the IR is quoted and escaped, and cut short.
    let mut ir = one_row_table(vec![txt("\u{1b}[2J\u{7}", 2), ph()]);
    ir["table"]["tbody"][0]["label"] =
        serde_json::json!({ "content": [{ "type": "text", "value": "x".repeat(40) }] });
    let o = bin()
        .arg("grid")
        .arg(write_json("grid-escapes", &ir))
        .output()
        .unwrap();
    let out = stdout(&o);
    assert!(!out.contains('\u{1b}') && !out.contains('\u{7}'), "{out}");
    assert!(
        out.contains(r#"A = cell(r0,c0, colspan=2) "\u{1b}[2J\u{7}""#),
        "{out}"
    );
    assert!(
        out.contains(&format!("tbody group 0 \"{}...\": 1 row", "x".repeat(21))),
        "{out}"
    );
}

/// Every command `gridwell --help` lists has a section in the CLI guide.
#[test]
fn every_command_is_documented() {
    let help = stdout(&bin().arg("--help").output().unwrap());
    let commands: Vec<&str> = help
        .split("Commands:\n")
        .nth(1)
        .unwrap()
        .lines()
        .take_while(|l| l.starts_with("  "))
        .map(|l| l.split_whitespace().next().unwrap())
        .filter(|&c| c != "help")
        .collect();
    assert_eq!(
        commands,
        [
            "convert",
            "validate",
            "fmt",
            "inspect",
            "grid",
            "conformance",
            "formats"
        ]
    );
    let guide = std::fs::read_to_string(repo("docs/guide/cli.qmd")).unwrap();
    for c in commands {
        assert!(guide.contains(&format!("\n### `gridwell {c}`\n")), "{c}");
    }
}
