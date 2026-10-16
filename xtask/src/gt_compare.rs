//! The "gt vs gridwell" page: gt's own HTML for real gt tables next to gridwell's
//! HTML for the same tables (via `gt_to_ir()`), both rendered in the same browser
//! with no harness styling, plus a pixel diff. Report-only: it measures how close
//! gridwell's default look is to gt's and never fails the run.
//!
//! The inputs are checked in under `harness/gt-reference/` (written by `make.R`
//! there), so the harness needs no R.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use gridwell_ir::Table;

use crate::diff::{self, Distance};
use crate::gallery::{esc, STYLE};
use crate::raster::{self, RasterOutcome, Tools};

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

enum Shot {
    Png(String),
    Unavailable(&'static str),
    Error(String),
}

struct Row {
    name: String,
    description: String,
    gt: Shot,
    gridwell: Shot,
    /// Relative path of the diff image, with the distance.
    diff: Option<(String, Distance)>,
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

    let shoot = |html: &Path, png: &Path| -> Shot {
        match raster::rasterize_bare_html(tools, html, png, work_dir) {
            RasterOutcome::Png(p) => {
                Shot::Png(format!("img/{}", p.file_name().unwrap().to_string_lossy()))
            }
            RasterOutcome::Unavailable(t) => Shot::Unavailable(t),
            RasterOutcome::Failed(e) => Shot::Error(e),
        }
    };

    let mut rows = Vec::new();
    for case in &reference.cases {
        let gt_out = out_dir.join(format!("{}.gt.html", case.name));
        fs::copy(&case.gt_html, &gt_out).map_err(|e| format!("copy {}: {e}", gt_out.display()))?;
        let gt_png = img_dir.join(format!("gt__{}__gt.png", case.name));
        let gt = shoot(&gt_out, &gt_png);

        let gw_out = out_dir.join(format!("{}.gridwell.html", case.name));
        let gridwell = match render_gridwell(&case.ir) {
            Ok(html) => {
                fs::write(&gw_out, html).map_err(|e| format!("write {}: {e}", gw_out.display()))?;
                shoot(
                    &gw_out,
                    &img_dir.join(format!("gt__{}__gridwell.png", case.name)),
                )
            }
            Err(e) => Shot::Error(e),
        };

        let diff = match (&gt, &gridwell) {
            (Shot::Png(a), Shot::Png(b)) => {
                let name = format!("gt__{}__diff.png", case.name);
                let d = diff::distance(&gallery.join(a), &gallery.join(b), &img_dir.join(&name))?;
                Some((format!("img/{name}"), d))
            }
            _ => None,
        };
        rows.push(Row {
            name: case.name.clone(),
            description: case.description.clone(),
            gt,
            gridwell,
            diff,
        });
    }

    let page = gallery.join("gt.html");
    fs::write(&page, render_page(&reference, &rows)).map_err(|e| format!("write gt.html: {e}"))?;
    eprintln!("gt comparison: {}", page.display());
    Ok(Some(summarize(&rows)))
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

fn summarize(rows: &[Row]) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "\n### gt vs gridwell HTML (report only)\n");
    let _ = writeln!(s, "| case | differing pixels | gt size | gridwell size |");
    let _ = writeln!(s, "|------|------------------|---------|---------------|");
    for r in rows {
        match &r.diff {
            Some((_, d)) => {
                let _ = writeln!(
                    s,
                    "| {} | {:.1}% | {}×{} | {}×{} |",
                    r.name,
                    d.fraction * 100.0,
                    d.size_a.0,
                    d.size_a.1,
                    d.size_b.0,
                    d.size_b.1
                );
            }
            None => {
                let _ = writeln!(s, "| {} | — | — | — |", r.name);
            }
        }
    }
    if let Some(mean) = mean_fraction(rows) {
        let _ = writeln!(s, "\nMean differing pixels: {:.1}%", mean * 100.0);
    }
    s
}

fn mean_fraction(rows: &[Row]) -> Option<f64> {
    let fs: Vec<f64> = rows
        .iter()
        .filter_map(|r| r.diff.as_ref().map(|(_, d)| d.fraction))
        .collect();
    (!fs.is_empty()).then(|| fs.iter().sum::<f64>() / fs.len() as f64)
}

fn render_page(reference: &Reference, rows: &[Row]) -> String {
    let mut h = String::new();
    h.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    h.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    h.push_str("<title>gt vs gridwell HTML</title>");
    h.push_str(STYLE);
    h.push_str(PAGE_STYLE);
    h.push_str("</head><body>");
    h.push_str("<p class=\"meta\"><a class=\"raw\" href=\"index.html\">← render gallery</a></p>");
    h.push_str("<h1>gt vs gridwell HTML</h1>");
    let _ = write!(
        h,
        "<p class=\"meta\">{} gt tables · references made with R {}, gt {}, gridwell {} \
         (<code>harness/gt-reference/make.R</code>)</p>",
        rows.len(),
        esc(&reference.r_version),
        esc(&reference.gt_version),
        esc(&reference.gridwell_version)
    );
    h.push_str(
        "<p class=\"legend\">Left: gt's own <code>as_raw_html()</code>. Middle: gridwell's \
         HTML for the same table (<code>gt_to_ir()</code>, default writer options). Both are \
         opened as bare pages in the same browser, with no harness styling, so this is what \
         a saved file looks like. Right: differing pixels in red; the score is the share of \
         differing pixels (lower is closer). Report only: nothing here fails CI.</p>",
    );
    if let Some(mean) = mean_fraction(rows) {
        let _ = write!(
            h,
            "<p class=\"meta\">Mean differing pixels: <b>{:.1}%</b></p>",
            mean * 100.0
        );
    }
    h.push_str(
        "<table class=\"grid\"><thead><tr><th class=\"exh\">case</th><th>gt</th>\
         <th>gridwell</th><th>diff</th></tr></thead><tbody>",
    );
    for r in rows {
        h.push_str("<tr><td class=\"exh\">");
        let _ = write!(
            h,
            "<div class=\"exname\">{}</div><div class=\"exdesc\">{}</div>",
            esc(&r.name),
            esc(&r.description)
        );
        let _ = write!(
            h,
            "<div class=\"files\"><a class=\"raw\" href=\"out/gt/{n}.gt.html\" target=\"_blank\" \
             rel=\"noopener\">gt html</a> · <a class=\"raw\" href=\"out/gt/{n}.gridwell.html\" \
             target=\"_blank\" rel=\"noopener\">gridwell html</a></div>",
            n = esc(&r.name)
        );
        h.push_str("</td>");
        for shot in [&r.gt, &r.gridwell] {
            h.push_str("<td>");
            h.push_str(&shot_html(shot));
            h.push_str("</td>");
        }
        h.push_str("<td>");
        match &r.diff {
            Some((rel, d)) => {
                let _ = write!(
                    h,
                    "<a href=\"{rel}\" target=\"_blank\" rel=\"noopener\"><img loading=\"lazy\" \
                     src=\"{rel}\"></a><span class=\"b score\">{:.1}% differ</span>\
                     <div class=\"sizes\">gt {}×{} · gridwell {}×{}</div>",
                    d.fraction * 100.0,
                    d.size_a.0,
                    d.size_a.1,
                    d.size_b.0,
                    d.size_b.1,
                    rel = esc(rel)
                );
            }
            None => h.push_str("<span class=\"b unavail\">no diff</span>"),
        }
        h.push_str("</td></tr>");
    }
    h.push_str("</tbody></table></body></html>");
    h
}

fn shot_html(shot: &Shot) -> String {
    match shot {
        Shot::Png(rel) => format!(
            "<a href=\"{r}\" target=\"_blank\" rel=\"noopener\"><img loading=\"lazy\" src=\"{r}\"></a>",
            r = esc(rel)
        ),
        Shot::Unavailable(t) => format!("<span class=\"b unavail\">no {}</span>", esc(t)),
        Shot::Error(e) => format!("<span class=\"b err\" title=\"{}\">error</span>", esc(e)),
    }
}

/// Larger previews than the main gallery: these are meant to be read.
const PAGE_STYLE: &str = "<style>
td a img { max-height: 520px; max-width: 460px; }
.files { margin-top: 8px; font-size: 11px; }
.sizes { font-size: 11px; color: #5b636c; margin-top: 4px; }
.score { background: #eef1f4; color: #33393f; }
code { font-size: 12px; }
</style>";

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
