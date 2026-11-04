//! The "gt vs gridwell" pages: a gt producer's own HTML for real tables next to
//! gridwell's HTML for the same tables (via the producer's IR emitter), both
//! rendered in the same browser with no harness styling, plus a pixel diff.
//! Report-only: they measure how close gridwell's default look is to gt's and
//! never fail the run.
//!
//! There is one page per producer (see [`SOURCES`]): gt in R and Great Tables in
//! Python. Their inputs are checked in under `harness/gt-reference/` and
//! `harness/gt-reference-py/` (written by the `make` script there), so the
//! harness needs neither R nor Python.

use std::fs;
use std::path::{Path, PathBuf};

use gridwell_ir::Table;

use crate::compare::{diff_shots, render_page, shoot, summarize, Page, Row, Shot};
use crate::gallery::esc;
use crate::raster::Tools;

/// A producer's reference set, and how its page is labeled.
pub struct Source {
    /// The references' directory, under the repository root.
    pub dir: &'static str,
    /// The script there that writes them.
    pub make: &'static str,
    /// The page, under `harness/gallery/`.
    pub page: &'static str,
    /// Names the page's files: `out/<key>/` and `img/<key>__<case>__*.png`.
    pub key: &'static str,
    pub title: &'static str,
    /// The producer, as the page and its summary name it.
    pub producer: &'static str,
    /// The calls that made the producer's HTML and the IR.
    pub html_call: &'static str,
    pub ir_call: &'static str,
    /// The manifest's version fields (label, key), in the order shown.
    pub versions: &'static [(&'static str, &'static str)],
}

/// gt (R): `harness/gt-reference`.
pub const GT: Source = Source {
    dir: "harness/gt-reference",
    make: "make.R",
    page: "gt.html",
    key: "gt",
    title: "gt vs gridwell HTML",
    producer: "gt",
    html_call: "as_raw_html()",
    ir_call: "gt_to_ir()",
    versions: &[
        ("R", "r_version"),
        ("gt", "gt_version"),
        ("gridwell", "gridwell_version"),
        // gt's own emitter (`gt::as_ir()`) or gridwell's copy.
        ("IR by", "ir_emitter"),
    ],
};

/// Great Tables (Python): `harness/gt-reference-py`, the same cases as gt's.
pub const GREAT_TABLES: Source = Source {
    dir: "harness/gt-reference-py",
    make: "make.py",
    page: "great-tables.html",
    key: "gtpy",
    title: "Great Tables vs gridwell HTML",
    producer: "Great Tables",
    html_call: "as_raw_html()",
    ir_call: "gridwell.gt_to_ir()",
    versions: &[
        ("Python", "python_version"),
        ("Great Tables", "great_tables_version"),
        ("gridwell", "gridwell_version"),
        // Great Tables' own emitter (`GT.as_ir()`) or gridwell's copy.
        ("IR by", "ir_emitter"),
    ],
};

pub const SOURCES: &[&Source] = &[&GT, &GREAT_TABLES];

/// One reference table.
pub struct Case {
    pub name: String,
    pub description: String,
    /// The producer's `as_raw_html()`.
    pub gt_html: PathBuf,
    /// The same table as gridwell IR.
    pub ir: PathBuf,
}

/// The manifest: versions the references were made with, and the cases.
pub struct Reference {
    /// (label, version), in the source's order.
    pub versions: Vec<(&'static str, String)>,
    pub cases: Vec<Case>,
}

impl Source {
    /// Read this source's references under `root`. See [`load`].
    pub fn load(&self, root: &Path) -> Result<Option<Reference>, String> {
        load(&root.join(self.dir), self)
    }
}

/// Read `dir/manifest.json`, whose version fields are `source`'s. `Ok(None)` if
/// there is no manifest; an error if it is malformed or names a case whose files
/// are missing.
pub fn load(dir: &Path, source: &Source) -> Result<Option<Reference>, String> {
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
    let versions = source
        .versions
        .iter()
        .map(|&(label, key)| Ok((label, field(&v, key)?)))
        .collect::<Result<_, String>>()?;
    Ok(Some(Reference { versions, cases }))
}

/// gridwell's HTML (default options) for a reference IR file.
pub fn render_gridwell(ir: &Path) -> Result<String, String> {
    let json = fs::read_to_string(ir).map_err(|e| format!("read {}: {e}", ir.display()))?;
    let table = Table::from_json(&json).map_err(|e| format!("parse {}: {e}", ir.display()))?;
    let out = gridwell_render::render(&table, "html", None)
        .map_err(|e| format!("render {}: {e}", ir.display()))?;
    Ok(out.as_text().expect("html is a text format").to_string())
}

/// What a source's page says about itself.
fn page<'a>(source: &'a Source, reference: &Reference, cases: usize, legend: &'a str) -> Page<'a> {
    let versions: Vec<String> = reference
        .versions
        .iter()
        .map(|(label, v)| format!("{label} {}", esc(v)))
        .collect();
    Page {
        title: source.title,
        meta_html: format!(
            "{cases} {} tables · references made with {} (<code>{}/{}</code>)",
            source.producer,
            versions.join(", "),
            source.dir,
            source.make
        ),
        legend_html: legend,
        left: source.producer,
        right: "gridwell",
    }
}

fn legend(source: &Source) -> String {
    format!(
        "Left: {}'s own <code>{}</code>. Middle: gridwell's HTML for the same table \
         (<code>{}</code>, default writer options). Both are opened as bare pages in the same \
         browser, with no harness styling, so this is what a saved file looks like. Right: \
         differing pixels in red; the score is the share of differing pixels (lower is \
         closer). Report only: nothing here fails CI.",
        source.producer, source.html_call, source.ir_call
    )
}

/// Build `gallery/<page>` for one source. Returns a Markdown summary for the run,
/// or `None` if it has no references.
pub fn run_source(
    source: &Source,
    root: &Path,
    tools: &Tools,
    gallery: &Path,
    work_dir: &Path,
) -> Result<Option<String>, String> {
    let Some(reference) = source.load(root)? else {
        return Ok(None);
    };
    let key = source.key;
    let out_dir = gallery.join("out").join(key);
    let img_dir = gallery.join("img");
    for d in [&out_dir, &img_dir] {
        fs::create_dir_all(d).map_err(|e| format!("mkdir {}: {e}", d.display()))?;
    }

    let mut rows = Vec::new();
    for case in &reference.cases {
        let gt_out = out_dir.join(format!("{}.gt.html", case.name));
        fs::copy(&case.gt_html, &gt_out).map_err(|e| format!("copy {}: {e}", gt_out.display()))?;
        let gt_png = img_dir.join(format!("{key}__{}__gt.png", case.name));
        let gt = shoot(tools, &gt_out, &gt_png, work_dir);

        let gw_out = out_dir.join(format!("{}.gridwell.html", case.name));
        let gridwell = match render_gridwell(&case.ir) {
            Ok(html) => {
                fs::write(&gw_out, html).map_err(|e| format!("write {}: {e}", gw_out.display()))?;
                shoot(
                    tools,
                    &gw_out,
                    &img_dir.join(format!("{key}__{}__gridwell.png", case.name)),
                    work_dir,
                )
            }
            Err(e) => Shot::Error(e),
        };

        let diff = diff_shots(gallery, &format!("{key}__{}", case.name), &gt, &gridwell)?;
        rows.push(Row {
            name: case.name.clone(),
            description: case.description.clone(),
            files: vec![
                (
                    format!("{} html", source.producer),
                    format!("out/{key}/{}.gt.html", case.name),
                ),
                (
                    "gridwell html".into(),
                    format!("out/{key}/{}.gridwell.html", case.name),
                ),
            ],
            left: gt,
            right: gridwell,
            diff,
        });
    }

    let legend = legend(source);
    let page = page(source, &reference, rows.len(), &legend);
    let path = gallery.join(source.page);
    fs::write(&path, render_page(&page, &rows))
        .map_err(|e| format!("write {}: {e}", source.page))?;
    eprintln!("{} comparison: {}", source.producer, path.display());
    Ok(Some(summarize(&page, &rows)))
}

/// Build every source's page. Returns the run's Markdown summary for those with
/// references, or `None` if none has any.
pub fn run(
    root: &Path,
    tools: &Tools,
    gallery: &Path,
    work_dir: &Path,
) -> Result<Option<String>, String> {
    let mut summary: Option<String> = None;
    for source in SOURCES {
        if let Some(s) = run_source(source, root, tools, gallery, work_dir)? {
            summary.get_or_insert_with(String::new).push_str(&s);
        }
    }
    Ok(summary)
}

/// `cargo xtask gt`: just these pages, without rendering the whole gallery.
pub fn run_alone(root: &Path) -> Result<String, String> {
    let gallery = root.join("harness/gallery");
    let work_dir = root.join("harness/.work");
    fs::create_dir_all(&work_dir).map_err(|e| format!("mkdir {}: {e}", work_dir.display()))?;
    let tools = Tools::detect();
    run(root, &tools, &gallery, &work_dir)?.ok_or_else(|| {
        let dirs: Vec<&str> = SOURCES.iter().map(|s| s.dir).collect();
        format!("no manifest.json in {}", dirs.join(" or "))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    fn reference(source: &Source) -> Reference {
        source
            .load(&root())
            .unwrap()
            .unwrap_or_else(|| panic!("{}/manifest.json", source.dir))
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gridwell-gtref-{tag}-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn manifest_lists_exactly_the_reference_files() {
        for source in SOURCES {
            let dir = root().join(source.dir);
            let reference = reference(source);
            assert!(reference.cases.len() >= 5, "{}", source.dir);
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
            assert_eq!(found, expected, "{}: orphan or missing files", source.dir);
            for c in &reference.cases {
                assert!(!c.description.is_empty(), "{}", c.name);
            }
            assert!(dir.join(source.make).exists(), "{}", source.make);
            for (label, v) in &reference.versions {
                assert!(!v.is_empty(), "{}: {label}", source.dir);
            }
        }
    }

    #[test]
    fn every_reference_renders_and_its_html_is_self_contained() {
        for source in SOURCES {
            for c in &reference(source).cases {
                let at = format!("{}/{}", source.dir, c.name);
                let html = render_gridwell(&c.ir).unwrap_or_else(|e| panic!("{e}"));
                assert!(html.contains("<table"), "{at}");
                let gt = fs::read_to_string(&c.gt_html).unwrap();
                // A fixed id (deterministic CSS scoping) and no external resources.
                assert!(gt.starts_with(&format!("<div id=\"{}\"", c.name)), "{at}");
                assert!(gt.contains("<style>") && gt.contains("<table"), "{at}");
                for external in ["@import", "<link", "<script", "http://", "https://"] {
                    assert!(!gt.contains(external), "{at}: {external}");
                }
            }
        }
    }

    #[test]
    fn the_great_tables_cases_mirror_the_gt_cases() {
        // So the two pages can be read side by side.
        let names = |s| -> Vec<String> { reference(s).cases.into_iter().map(|c| c.name).collect() };
        assert_eq!(names(&GREAT_TABLES), names(&GT));
    }

    #[test]
    fn sources_write_distinct_files() {
        for (i, a) in SOURCES.iter().enumerate() {
            for b in &SOURCES[i + 1..] {
                assert_ne!(a.dir, b.dir);
                assert_ne!(a.page, b.page);
                assert_ne!(a.key, b.key);
            }
        }
        // Every reference set in harness/ has a page.
        let mut dirs: Vec<String> = fs::read_dir(root().join("harness"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.join("manifest.json").exists())
            .map(|p| format!("harness/{}", p.file_name().unwrap().to_string_lossy()))
            .collect();
        dirs.sort();
        let mut sources: Vec<&str> = SOURCES.iter().map(|s| s.dir).collect();
        sources.sort();
        assert_eq!(dirs, sources);
        // The pages the gallery's index links to.
        assert_eq!(GT.page, "gt.html");
        assert_eq!(GREAT_TABLES.page, "great-tables.html");
    }

    #[test]
    fn the_page_names_its_versions_and_script() {
        let reference = reference(&GREAT_TABLES);
        let legend = legend(&GREAT_TABLES);
        let page = page(&GREAT_TABLES, &reference, 9, &legend);
        assert_eq!(page.left, "Great Tables");
        assert!(
            page.meta_html.starts_with("9 Great Tables tables"),
            "{}",
            page.meta_html
        );
        let version = |label| {
            let (_, v) = reference
                .versions
                .iter()
                .find(|(l, _)| *l == label)
                .unwrap();
            format!("{label} {v}")
        };
        for label in ["Python", "Great Tables", "gridwell", "IR by"] {
            assert!(
                page.meta_html.contains(&version(label)),
                "{}",
                page.meta_html
            );
        }
        assert!(page.meta_html.contains("harness/gt-reference-py/make.py"));
        assert!(legend.contains("gridwell.gt_to_ir()"), "{legend}");
    }

    #[test]
    fn a_missing_manifest_is_no_reference() {
        let d = temp_dir("none");
        for source in SOURCES {
            assert!(load(&d, source).unwrap().is_none());
        }
    }

    #[test]
    fn a_case_without_its_files_is_an_error() {
        let d = temp_dir("bad");
        fs::write(
            d.join("manifest.json"),
            r#"{"r_version":"4","gt_version":"1","gridwell_version":"0","ir_emitter":"gridwell",
                "cases":[{"name":"ghost","description":"x"}]}"#,
        )
        .unwrap();
        let err = load(&d, &GT).err().unwrap();
        assert!(err.contains("ghost.gt.html"), "{err}");
    }

    #[test]
    fn a_manifest_needs_its_sources_version_fields() {
        let d = temp_dir("versions");
        fs::write(
            d.join("manifest.json"),
            r#"{"r_version":"4","gt_version":"1","gridwell_version":"0","ir_emitter":"gt",
                "cases":[]}"#,
        )
        .unwrap();
        assert!(load(&d, &GT).unwrap().is_some());
        let err = load(&d, &GREAT_TABLES).err().unwrap();
        assert!(err.contains("\"python_version\""), "{err}");
    }
}
