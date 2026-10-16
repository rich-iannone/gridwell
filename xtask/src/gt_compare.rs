//! The "gt vs gridwell" page: gt's own HTML for real gt tables next to gridwell's
//! HTML for the same tables (via `gt_to_ir()`), both rendered in the same browser
//! with no harness styling, plus a pixel diff. Report-only: it measures how close
//! gridwell's default look is to gt's and never fails the run.
//!
//! The inputs are checked in under `harness/gt-reference/` (written by `make.R`
//! there), so the harness needs no R.

use std::fs;
use std::path::{Path, PathBuf};

use gridwell_ir::Table;

use crate::compare::{diff_shots, render_page, shoot, summarize, Page, Row, Shot};
use crate::gallery::esc;
use crate::raster::Tools;

/// One reference table.
pub struct Case {
    pub name: String,
    pub description: String,
    /// gt's `as_raw_html()`.
    pub gt_html: PathBuf,
    /// The same table as gridwell IR.
    pub ir: PathBuf,
}

/// The manifest: versions the references were made with, and the cases.
pub struct Reference {
    pub r_version: String,
    pub gt_version: String,
    pub gridwell_version: String,
    pub cases: Vec<Case>,
}

/// Read `dir/manifest.json`. `Ok(None)` if there is no manifest; an error if it is
/// malformed or names a case whose files are missing.
pub fn load(dir: &Path) -> Result<Option<Reference>, String> {
    let path = dir.join("manifest.json");
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;
    let field = |v: &serde_json::Value, k: &str| -> Result<String, String> {
        v.get(k)
            .and_then(|x| x.as_str())
            .map(str::to_string)
            .ok_or_else(|| format!("{}: missing string field {k:?}", path.display()))
    };
    let mut cases = Vec::new();
    for c in v
        .get("cases")
        .and_then(|c| c.as_array())
        .ok_or_else(|| format!("{}: missing \"cases\" array", path.display()))?
    {
        let name = field(c, "name")?;
        let case = Case {
            description: field(c, "description")?,
            gt_html: dir.join(format!("{name}.gt.html")),
            ir: dir.join(format!("{name}.ir.json")),
            name,
        };
        for p in [&case.gt_html, &case.ir] {
            if !p.exists() {
                return Err(format!("{}: {} is missing", path.display(), p.display()));
            }
        }
        cases.push(case);
    }
    Ok(Some(Reference {
        r_version: field(&v, "r_version")?,
        gt_version: field(&v, "gt_version")?,
        gridwell_version: field(&v, "gridwell_version")?,
        cases,
    }))
}

/// gridwell's HTML (default options) for a reference IR file.
pub fn render_gridwell(ir: &Path) -> Result<String, String> {
    let json = fs::read_to_string(ir).map_err(|e| format!("read {}: {e}", ir.display()))?;
    let table = Table::from_json(&json).map_err(|e| format!("parse {}: {e}", ir.display()))?;
    let out = gridwell_render::render(&table, "html", None)
        .map_err(|e| format!("render {}: {e}", ir.display()))?;
    Ok(out.as_text().expect("html is a text format").to_string())
}

/// What the gt page says about itself.
fn page(reference: &Reference, cases: usize) -> Page<'static> {
    Page {
        title: "gt vs gridwell HTML",
        meta_html: format!(
            "{cases} gt tables · references made with R {}, gt {}, gridwell {} \
             (<code>harness/gt-reference/make.R</code>)",
            esc(&reference.r_version),
            esc(&reference.gt_version),
            esc(&reference.gridwell_version)
        ),
        legend_html: "Left: gt's own <code>as_raw_html()</code>. Middle: gridwell's \
         HTML for the same table (<code>gt_to_ir()</code>, default writer options). Both are \
         opened as bare pages in the same browser, with no harness styling, so this is what \
         a saved file looks like. Right: differing pixels in red; the score is the share of \
         differing pixels (lower is closer). Report only: nothing here fails CI.",
        left: "gt",
        right: "gridwell",
    }
}

/// Build `gallery/gt.html` from `harness/gt-reference`. Returns a Markdown summary
/// for the run, or `None` if there are no references.
pub fn run(
    root: &Path,
    tools: &Tools,
    gallery: &Path,
    work_dir: &Path,
) -> Result<Option<String>, String> {
    let Some(reference) = load(&root.join("harness/gt-reference"))? else {
        return Ok(None);
    };
    let out_dir = gallery.join("out/gt");
    let img_dir = gallery.join("img");
    for d in [&out_dir, &img_dir] {
        fs::create_dir_all(d).map_err(|e| format!("mkdir {}: {e}", d.display()))?;
    }

    let mut rows = Vec::new();
    for case in &reference.cases {
        let gt_out = out_dir.join(format!("{}.gt.html", case.name));
        fs::copy(&case.gt_html, &gt_out).map_err(|e| format!("copy {}: {e}", gt_out.display()))?;
        let gt_png = img_dir.join(format!("gt__{}__gt.png", case.name));
        let gt = shoot(tools, &gt_out, &gt_png, work_dir);

        let gw_out = out_dir.join(format!("{}.gridwell.html", case.name));
        let gridwell = match render_gridwell(&case.ir) {
            Ok(html) => {
                fs::write(&gw_out, html).map_err(|e| format!("write {}: {e}", gw_out.display()))?;
                shoot(
                    tools,
                    &gw_out,
                    &img_dir.join(format!("gt__{}__gridwell.png", case.name)),
                    work_dir,
                )
            }
            Err(e) => Shot::Error(e),
        };

        let diff = diff_shots(gallery, &format!("gt__{}", case.name), &gt, &gridwell)?;
        rows.push(Row {
            name: case.name.clone(),
            description: case.description.clone(),
            files: vec![
                ("gt html".into(), format!("out/gt/{}.gt.html", case.name)),
                (
                    "gridwell html".into(),
                    format!("out/gt/{}.gridwell.html", case.name),
                ),
            ],
            left: gt,
            right: gridwell,
            diff,
        });
    }

    let page = page(&reference, rows.len());
    let path = gallery.join("gt.html");
    fs::write(&path, render_page(&page, &rows)).map_err(|e| format!("write gt.html: {e}"))?;
    eprintln!("gt comparison: {}", path.display());
    Ok(Some(summarize(&page, &rows)))
}

/// `cargo xtask gt`: just this page, without rendering the whole gallery.
pub fn run_alone(root: &Path) -> Result<String, String> {
    let gallery = root.join("harness/gallery");
    let work_dir = root.join("harness/.work");
    fs::create_dir_all(&work_dir).map_err(|e| format!("mkdir {}: {e}", work_dir.display()))?;
    let tools = Tools::detect();
    run(root, &tools, &gallery, &work_dir)?
        .ok_or_else(|| "no harness/gt-reference/manifest.json".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../harness/gt-reference")
    }

    #[test]
    fn manifest_lists_exactly_the_reference_files() {
        let dir = reference_dir();
        let reference = load(&dir)
            .unwrap()
            .expect("harness/gt-reference/manifest.json");
        assert!(reference.cases.len() >= 5);
        let mut expected: Vec<String> = reference
            .cases
            .iter()
            .flat_map(|c| [format!("{}.gt.html", c.name), format!("{}.ir.json", c.name)])
            .collect();
        expected.sort();
        let mut found: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".gt.html") || n.ends_with(".ir.json"))
            .collect();
        found.sort();
        assert_eq!(found, expected, "orphan or missing reference files");
        for c in &reference.cases {
            assert!(!c.description.is_empty(), "{}", c.name);
        }
    }

    #[test]
    fn every_reference_renders_and_gt_html_is_self_contained() {
        let reference = load(&reference_dir()).unwrap().unwrap();
        for c in &reference.cases {
            let html = render_gridwell(&c.ir).unwrap_or_else(|e| panic!("{e}"));
            assert!(html.contains("<table"), "{}", c.name);
            let gt = fs::read_to_string(&c.gt_html).unwrap();
            // A fixed id (deterministic CSS scoping) and no external resources.
            assert!(
                gt.starts_with(&format!("<div id=\"{}\"", c.name)),
                "{}",
                c.name
            );
            assert!(
                gt.contains("<style>") && gt.contains("<table"),
                "{}",
                c.name
            );
            for external in ["@import", "<link", "<script", "http://", "https://"] {
                assert!(!gt.contains(external), "{}: {external}", c.name);
            }
        }
    }

    #[test]
    fn a_missing_manifest_is_no_reference() {
        let d = std::env::temp_dir().join(format!("gridwell-gtref-none-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        assert!(load(&d).unwrap().is_none());
    }

    #[test]
    fn a_case_without_its_files_is_an_error() {
        let d = std::env::temp_dir().join(format!("gridwell-gtref-bad-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join("manifest.json"),
            r#"{"r_version":"4","gt_version":"1","gridwell_version":"0",
                "cases":[{"name":"ghost","description":"x"}]}"#,
        )
        .unwrap();
        let err = load(&d).err().unwrap();
        assert!(err.contains("ghost.gt.html"), "{err}");
    }
}
