//! Side-by-side comparison pages (report only): two renderings of the same table,
//! each screenshotted as a bare page in the same browser, plus a pixel diff. Used by
//! the gt page (gt's HTML vs gridwell's) and the SVG page (gridwell's HTML vs its
//! SVG).

use std::fmt::Write as _;
use std::path::Path;

use crate::diff::{self, Distance};
use crate::gallery::{esc, STYLE};
use crate::raster::{self, RasterOutcome, Tools};

/// One screenshot, as a path relative to the gallery.
pub enum Shot {
    Png(String),
    Unavailable(&'static str),
    Error(String),
}

/// Screenshot an HTML fragment (or inline SVG) as a bare page.
pub fn shoot(tools: &Tools, html: &Path, png: &Path, work_dir: &Path) -> Shot {
    match raster::rasterize_bare_html(tools, html, png, work_dir) {
        RasterOutcome::Png(p) => {
            Shot::Png(format!("img/{}", p.file_name().unwrap().to_string_lossy()))
        }
        RasterOutcome::Unavailable(t) => Shot::Unavailable(t),
        RasterOutcome::Failed(e) => Shot::Error(e),
    }
}

/// One compared table.
pub struct Row {
    pub name: String,
    pub description: String,
    /// Links shown under the name: (label, path relative to the gallery).
    pub files: Vec<(String, String)>,
    pub left: Shot,
    pub right: Shot,
    /// Relative path of the diff image, with the distance.
    pub diff: Option<(String, Distance)>,
}

/// Diff two screenshots into `img/{stem}__diff.png`, if both exist.
pub fn diff_shots(
    gallery: &Path,
    stem: &str,
    left: &Shot,
    right: &Shot,
) -> Result<Option<(String, Distance)>, String> {
    let (Shot::Png(a), Shot::Png(b)) = (left, right) else {
        return Ok(None);
    };
    let rel = format!("img/{stem}__diff.png");
    let d = diff::distance(&gallery.join(a), &gallery.join(b), &gallery.join(&rel))?;
    Ok(Some((rel, d)))
}

/// What a page says about itself.
pub struct Page<'a> {
    pub title: &'a str,
    /// HTML for the line under the title (counts, versions).
    pub meta_html: String,
    /// HTML for the explanation paragraph.
    pub legend_html: &'a str,
    pub left: &'a str,
    pub right: &'a str,
}

pub fn mean_fraction(rows: &[Row]) -> Option<f64> {
    let fs: Vec<f64> = rows
        .iter()
        .filter_map(|r| r.diff.as_ref().map(|(_, d)| d.fraction))
        .collect();
    (!fs.is_empty()).then(|| fs.iter().sum::<f64>() / fs.len() as f64)
}

/// The Markdown summary for the run.
pub fn summarize(page: &Page, rows: &[Row]) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "\n### {} (report only)\n", page.title);
    let _ = writeln!(
        s,
        "| case | differing pixels | {} size | {} size |",
        page.left, page.right
    );
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

pub fn render_page(page: &Page, rows: &[Row]) -> String {
    let mut h = String::new();
    h.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    h.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    let _ = write!(h, "<title>{}</title>", esc(page.title));
    h.push_str(STYLE);
    h.push_str(PAGE_STYLE);
    h.push_str("</head><body>");
    h.push_str("<p class=\"meta\"><a class=\"raw\" href=\"index.html\">← render gallery</a></p>");
    let _ = write!(h, "<h1>{}</h1>", esc(page.title));
    let _ = write!(h, "<p class=\"meta\">{}</p>", page.meta_html);
    let _ = write!(h, "<p class=\"legend\">{}</p>", page.legend_html);
    if let Some(mean) = mean_fraction(rows) {
        let _ = write!(
            h,
            "<p class=\"meta\">Mean differing pixels: <b>{:.1}%</b></p>",
            mean * 100.0
        );
    }
    let _ = write!(
        h,
        "<table class=\"grid\"><thead><tr><th class=\"exh\">case</th><th>{}</th>\
         <th>{}</th><th>diff</th></tr></thead><tbody>",
        esc(page.left),
        esc(page.right)
    );
    for r in rows {
        h.push_str("<tr><td class=\"exh\">");
        let _ = write!(
            h,
            "<div class=\"exname\">{}</div><div class=\"exdesc\">{}</div>",
            esc(&r.name),
            esc(&r.description)
        );
        if !r.files.is_empty() {
            let links: Vec<String> = r
                .files
                .iter()
                .map(|(label, href)| {
                    format!(
                        "<a class=\"raw\" href=\"{}\" target=\"_blank\" rel=\"noopener\">{}</a>",
                        esc(href),
                        esc(label)
                    )
                })
                .collect();
            let _ = write!(h, "<div class=\"files\">{}</div>", links.join(" · "));
        }
        h.push_str("</td>");
        for shot in [&r.left, &r.right] {
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
                     <div class=\"sizes\">{} {}×{} · {} {}×{}</div>",
                    d.fraction * 100.0,
                    esc(page.left),
                    d.size_a.0,
                    d.size_a.1,
                    esc(page.right),
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
