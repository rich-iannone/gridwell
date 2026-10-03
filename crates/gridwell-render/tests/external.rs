//! Oracles that need external programs:
//!
//! - **pandoc** must accept the Pandoc and Quarto output (as a document), and the
//!   HTML it renders from them must show the IR's grid: a round trip through a
//!   second, independent implementation of the Pandoc table model.
//! - **LibreOffice** must open every RTF, DOCX, XLSX and PPTX file and convert it to
//!   PDF.
//!
//! Each test skips when its tool is missing, unless `GRIDWELL_REQUIRE_PANDOC` /
//! `GRIDWELL_REQUIRE_SOFFICE` is set (as in CI's `oracles` job).

mod support;

use std::path::PathBuf;
use std::process::{Command, Stdio};

use gridwell_render::render;
use gridwell_testkit::examples;
use support::readers::html_grid;
use support::{compare, expected};

fn require(tool: &str, var: &str) -> bool {
    if Command::new(tool).arg("--version").output().is_ok() {
        return true;
    }
    if std::env::var_os(var).is_some_and(|v| !v.is_empty()) {
        panic!("{tool} not found but {var} is set");
    }
    eprintln!("skipping: {tool} not found");
    false
}

fn work_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gridwell-oracle-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Our output is one block; pandoc reads whole documents.
fn as_document(block_json: &str) -> String {
    let block: serde_json::Value = serde_json::from_str(block_json).unwrap();
    serde_json::json!({
        "pandoc-api-version": [1, 23, 1],
        "meta": {},
        "blocks": [block]
    })
    .to_string()
}

/// Run pandoc on a JSON document; `Err` carries its stderr.
fn pandoc(doc: &str, to: &str) -> Result<String, String> {
    use std::io::Write;
    let mut child = Command::new("pandoc")
        .args(["-f", "json", "-t", to])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(doc.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

#[test]
fn pandoc_reads_our_ast_and_renders_the_ir_grid() {
    if !require("pandoc", "GRIDWELL_REQUIRE_PANDOC") {
        return;
    }
    let mut failures = Vec::new();
    for ex in examples() {
        let t = ex.table();
        let want = expected(&t);
        for format in ["pandoc", "quarto"] {
            let out = render(&t, format, None).unwrap();
            let doc = as_document(out.as_text().unwrap());
            // pandoc must accept the AST at all (`native` is a lossless dump).
            if let Err(e) = pandoc(&doc, "native") {
                failures.push(format!(
                    "{format} / {}: pandoc rejected the AST: {e}",
                    ex.name
                ));
                continue;
            }
            match pandoc(&doc, "html") {
                Ok(html) => {
                    if let Err(e) = compare(&html_grid(&html), &want, false) {
                        failures.push(format!("{format} / {} via pandoc HTML: {e}", ex.name));
                    }
                }
                Err(e) => failures.push(format!("{format} / {}: {e}", ex.name)),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn libreoffice_converts_every_office_file_to_pdf() {
    // `soffice --version` would start the whole suite; probe the binary instead.
    let found = Command::new("soffice")
        .arg("--help")
        .stdout(Stdio::null())
        .output()
        .is_ok();
    if !found {
        if std::env::var_os("GRIDWELL_REQUIRE_SOFFICE").is_some_and(|v| !v.is_empty()) {
            panic!("soffice not found but GRIDWELL_REQUIRE_SOFFICE is set");
        }
        eprintln!("skipping: soffice not found");
        return;
    }
    let dir = work_dir("soffice");
    let mut inputs = Vec::new();
    for ex in examples() {
        let t = ex.table();
        for (format, ext) in [
            ("rtf", "rtf"),
            ("docx", "docx"),
            ("xlsx", "xlsx"),
            ("pptx", "pptx"),
        ] {
            // A distinct stem per format: LibreOffice names the PDF after the stem.
            let path = dir.join(format!("{}-{format}.{ext}", ex.name));
            std::fs::write(&path, render(&t, format, None).unwrap().into_bytes()).unwrap();
            inputs.push(path);
        }
    }
    // One LibreOffice start for all files (it takes seconds to start).
    let out_dir = dir.join("pdf");
    let status = Command::new("soffice")
        .args([
            "--headless",
            "--norestore",
            "--convert-to",
            "pdf",
            "--outdir",
        ])
        .arg(&out_dir)
        .args(&inputs)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let missing: Vec<String> = inputs
        .iter()
        .filter(|p| {
            let pdf = out_dir.join(p.file_stem().unwrap()).with_extension("pdf");
            // A real page, not an empty or truncated file.
            std::fs::metadata(&pdf)
                .map(|m| m.len() < 500)
                .unwrap_or(true)
        })
        .map(|p| p.display().to_string())
        .collect();
    assert!(missing.is_empty(), "no PDF for: {missing:?}");
}
