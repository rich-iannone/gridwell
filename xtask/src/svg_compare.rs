//! The "HTML vs SVG" page: gridwell's HTML next to its SVG for the same table, both
//! with default options and screenshotted as bare pages in the same browser, plus a
//! pixel diff. Report only. The HTML look (within ~1% of gt, see the gt page) is
//! the target for the SVG writer.
//!
//! Tables: every corpus example and every gt reference (`harness/gt-reference`).

use std::fs;
use std::path::Path;

use gridwell_ir::Table;
use gridwell_testkit::examples;

use crate::compare::{diff_shots, render_page, shoot, summarize, Page, Row, Shot};
use crate::raster::Tools;

/// One table to compare.
struct Case {
    name: String,
    description: String,
    table: Result<Table, String>,
}

fn cases(root: &Path) -> Result<Vec<Case>, String> {
    let mut out: Vec<Case> = examples()
        .into_iter()
        .map(|ex| Case {
            name: ex.name.to_string(),
            description: ex.description.to_string(),
            table: Ok(ex.table()),
        })
        .collect();
    if let Some(reference) = crate::gt_compare::load(&root.join("harness/gt-reference"))? {
        for c in reference.cases {
            let table = fs::read_to_string(&c.ir)
                .map_err(|e| format!("read {}: {e}", c.ir.display()))
                .and_then(|json| Table::from_json(&json).map_err(|e| e.to_string()));
            out.push(Case {
                name: format!("gt-{}", c.name),
                description: format!("gt reference: {}", c.description),
                table,
            });
        }
    }
    Ok(out)
}

fn render(table: &Table, format: &str) -> Result<String, String> {
    gridwell_render::render(table, format, None)
        .map_err(|e| format!("render {format}: {e}"))
        .map(|o| o.as_text().expect("text format").to_string())
}

/// SVG for inlining in an HTML page: without the XML declaration, which is only
/// meaningful at the start of a standalone file.
pub fn inline_svg(svg: &str) -> &str {
    let svg = svg.trim_start();
    match svg.strip_prefix("<?xml") {
        Some(rest) => rest.split_once("?>").map_or(svg, |(_, r)| r.trim_start()),
        None => svg,
    }
}

fn page(cases: usize) -> Page<'static> {
    Page {
        title: "gridwell HTML vs SVG",
        meta_html: format!(
            "{cases} tables (the corpus and the gt references) · default writer options"
        ),
        legend_html: "Left: gridwell's HTML (whose look is within ~1% of gt; see the \
         <a class=\"raw\" href=\"gt.html\">gt page</a>). Middle: gridwell's SVG for the same \
         table. Both are opened as bare pages in the same browser (the SVG inline), with no \
         harness styling. Right: differing pixels in red; the score is the share of differing \
         pixels (lower is closer). Report only: nothing here fails CI.",
        left: "html",
        right: "svg",
    }
}

/// Build `gallery/svg.html`. Returns a Markdown summary for the run.
pub fn run(root: &Path, tools: &Tools, gallery: &Path, work_dir: &Path) -> Result<String, String> {
    let out_dir = gallery.join("out/svg-compare");
    let img_dir = gallery.join("img");
    for d in [&out_dir, &img_dir] {
        fs::create_dir_all(d).map_err(|e| format!("mkdir {}: {e}", d.display()))?;
    }
    let write = |path: &Path, text: &str| {
        fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
    };

    let mut rows = Vec::new();
    for case in cases(root)? {
        let stem = format!("svgcmp__{}", case.name);
        let html_path = out_dir.join(format!("{}.html", case.name));
        let svg_path = out_dir.join(format!("{}.svg", case.name));
        let svg_page = out_dir.join(format!("{}.svg.html", case.name));
        let (left, right) = match &case.table {
            Err(e) => (Shot::Error(e.clone()), Shot::Error(e.clone())),
            Ok(table) => {
                let left = match render(table, "html") {
                    Ok(html) => {
                        write(&html_path, &html)?;
                        shoot(
                            tools,
                            &html_path,
                            &img_dir.join(format!("{stem}__html.png")),
                            work_dir,
                        )
                    }
                    Err(e) => Shot::Error(e),
                };
                let right = match render(table, "svg") {
                    Ok(svg) => {
                        write(&svg_path, &svg)?;
                        write(&svg_page, inline_svg(&svg))?;
                        shoot(
                            tools,
                            &svg_page,
                            &img_dir.join(format!("{stem}__svg.png")),
                            work_dir,
                        )
                    }
                    Err(e) => Shot::Error(e),
                };
                (left, right)
            }
        };
        let diff = diff_shots(gallery, &stem, &left, &right)?;
        rows.push(Row {
            files: vec![
                ("html".into(), format!("out/svg-compare/{}.html", case.name)),
                ("svg".into(), format!("out/svg-compare/{}.svg", case.name)),
            ],
            name: case.name,
            description: case.description,
            left,
            right,
            diff,
        });
    }

    let page = page(rows.len());
    let path = gallery.join("svg.html");
    write(&path, &render_page(&page, &rows))?;
    eprintln!("svg comparison: {}", path.display());
    Ok(summarize(&page, &rows))
}

/// `cargo xtask svg`: just this page.
pub fn run_alone(root: &Path) -> Result<String, String> {
    let gallery = root.join("harness/gallery");
    let work_dir = root.join("harness/.work");
    fs::create_dir_all(&work_dir).map_err(|e| format!("mkdir {}: {e}", work_dir.display()))?;
    run(root, &Tools::detect(), &gallery, &work_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_svg_drops_only_the_xml_declaration() {
        let svg = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"x\"></svg>";
        assert_eq!(inline_svg(svg), "<svg xmlns=\"x\"></svg>");
        assert_eq!(inline_svg("<svg/>"), "<svg/>");
        assert_eq!(inline_svg("  <svg/>"), "<svg/>");
    }

    #[test]
    fn every_case_renders_to_html_and_svg() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let cases = cases(&root).unwrap();
        assert!(cases.len() > examples().len(), "gt references included");
        let mut names = std::collections::HashSet::new();
        for c in &cases {
            assert!(names.insert(c.name.clone()), "duplicate case {}", c.name);
            let t = c
                .table
                .as_ref()
                .unwrap_or_else(|e| panic!("{}: {e}", c.name));
            assert!(render(t, "html").unwrap().contains("<table"), "{}", c.name);
            let svg = render(t, "svg").unwrap();
            assert!(inline_svg(&svg).starts_with("<svg"), "{}", c.name);
        }
    }
}
