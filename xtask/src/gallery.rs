//! Render every example to every format, rasterize to previews, compare the
//! gated subset to goldens, and emit a browsable `index.html`.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use gridwell_testkit::examples;

use crate::diff::{self, Comparison};
use crate::formats::{Output, FORMATS};
use crate::raster::{self, RasterOutcome, Tools};

/// Per-cell result used to build the gallery and the summary.
enum CellStatus {
    Png {
        rel: String,
        diff: Option<Comparison>,
    },
    Text {
        preview: String,
    },
    Unavailable(&'static str),
    Error(String),
}

struct Cell {
    format_id: &'static str,
    rel_out: Option<String>,
    status: CellStatus,
}

struct GalleryRow {
    name: &'static str,
    category: &'static str,
    description: &'static str,
    tags: Vec<&'static str>,
    cells: Vec<Cell>,
}

/// Run the gallery build. Returns `true` if a gated regression was found.
pub fn run(root: &Path, check: bool, accept: bool) -> Result<bool, String> {
    let gallery = root.join("harness/gallery");
    let out_dir = gallery.join("out");
    let img_dir = gallery.join("img");
    // Temp scratch for the rasterizers lives OUTSIDE the published gallery dir
    // so only index.html + img/ + out/ are served on GitHub Pages.
    let work_dir = root.join("harness/.work");
    for d in [&out_dir, &img_dir, &work_dir] {
        fs::create_dir_all(d).map_err(|e| format!("mkdir {}: {e}", d.display()))?;
    }
    let goldens = root.join("harness/goldens");
    // Every gated render, laid out like harness/goldens. CI uploads it as an artifact,
    // so blessing goldens never requires a local Docker toolchain: download, copy over
    // harness/goldens, commit.
    let proposed = root.join("harness/proposed-goldens");
    if proposed.exists() {
        fs::remove_dir_all(&proposed).map_err(|e| format!("clean {}: {e}", proposed.display()))?;
    }

    let tools = Tools::detect();
    eprintln!("renderers: {}", tools.summary());

    let mut rows = Vec::new();
    for ex in examples() {
        let table = ex.table();
        let mut cells = Vec::new();
        for fmt in FORMATS {
            let out_name = format!("{}.{}", ex.name, fmt.ext());
            let out_path = out_dir.join(&out_name);
            let rel_out = format!("out/{out_name}");

            // 1. Render the format to disk.
            let bytes = match fmt.render(&table) {
                Ok(Output::Text(s)) => s.into_bytes(),
                Ok(Output::Bytes(b)) => b,
                Err(e) => {
                    cells.push(Cell {
                        format_id: fmt.id,
                        rel_out: None,
                        status: CellStatus::Error(format!("render: {e}")),
                    });
                    continue;
                }
            };
            if let Err(e) = fs::write(&out_path, &bytes) {
                return Err(format!("write {}: {e}", out_path.display()));
            }

            // 2. Text formats are shown verbatim; others are rasterized.
            let status = match fmt.raster {
                crate::formats::Raster::AnsiText => {
                    let text = raster::strip_ansi(&String::from_utf8_lossy(&bytes));
                    CellStatus::Text {
                        preview: truncate(&text, 2000),
                    }
                }
                crate::formats::Raster::SourceText => CellStatus::Text {
                    preview: truncate(&String::from_utf8_lossy(&bytes), 2000),
                },
                kind => {
                    let png_path = img_dir.join(format!("{}__{}.png", ex.name, fmt.id));
                    match raster::rasterize(kind, &tools, &out_path, &png_path, &work_dir) {
                        RasterOutcome::Png(p) => {
                            let rel = format!("img/{}", p.file_name().unwrap().to_string_lossy());
                            let diff = if fmt.gated {
                                let golden = goldens.join(fmt.id).join(format!("{}.png", ex.name));
                                copy_golden(
                                    &p,
                                    &proposed.join(fmt.id).join(format!("{}.png", ex.name)),
                                )?;
                                if accept {
                                    copy_golden(&p, &golden)?;
                                    Some(Comparison::Unchanged)
                                } else if check {
                                    Some(diff::compare(&p, &golden)?)
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                            CellStatus::Png { rel, diff }
                        }
                        RasterOutcome::Unavailable(t) => CellStatus::Unavailable(t),
                        RasterOutcome::Failed(e) => CellStatus::Error(e),
                    }
                }
            };

            cells.push(Cell {
                format_id: fmt.id,
                rel_out: Some(rel_out),
                status,
            });
        }
        rows.push(GalleryRow {
            name: ex.name,
            category: ex.category.slug(),
            description: ex.description,
            tags: ex.tags.iter().map(|t| t.slug()).collect(),
            cells,
        });
    }

    let index = gallery.join("index.html");
    fs::write(&index, render_html(&rows, &tools)).map_err(|e| format!("write index: {e}"))?;
    eprintln!("gallery: {}", index.display());

    let stale = if check {
        stale_goldens(&goldens, &rows)?
    } else {
        Vec::new()
    };
    let (regressions, summary) = summarize(&rows, &stale, check, accept);
    eprint!("{summary}");
    write_gh_summary(&summary);

    Ok(regressions)
}

fn copy_golden(render: &Path, golden: &Path) -> Result<(), String> {
    if let Some(parent) = golden.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    fs::copy(render, golden).map_err(|e| format!("copy golden {}: {e}", golden.display()))?;
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}\n… (truncated)")
    }
}

// ─────────────────────────────── summary ─────────────────────────────────

/// Why a gated cell fails `--check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GateFailure {
    /// Differs from the golden beyond tolerance.
    Changed,
    /// Golden and render have different dimensions.
    SizeMismatch,
    /// No golden committed for this example/format.
    MissingGolden,
    /// The gated format could not be rendered or rasterized. The pinned image has
    /// every gated renderer, so this is a failure, not a graceful degradation.
    NotRendered,
}

impl GateFailure {
    fn label(self) -> &'static str {
        match self {
            GateFailure::Changed => "changed",
            GateFailure::SizeMismatch => "size changed",
            GateFailure::MissingGolden => "missing golden",
            GateFailure::NotRendered => "not rendered",
        }
    }
}

/// The `--check` verdict for one cell of a gated format (`None` = passes).
fn gate_failure(status: &CellStatus) -> Option<GateFailure> {
    match status {
        CellStatus::Png { diff, .. } => match diff {
            Some(Comparison::Changed { .. }) => Some(GateFailure::Changed),
            Some(Comparison::SizeMismatch) => Some(GateFailure::SizeMismatch),
            Some(Comparison::New) => Some(GateFailure::MissingGolden),
            Some(Comparison::Unchanged) | None => None,
        },
        CellStatus::Unavailable(_) | CellStatus::Error(_) => Some(GateFailure::NotRendered),
        // Gated formats are always rasterized; a text cell would be a harness bug.
        CellStatus::Text { .. } => Some(GateFailure::NotRendered),
    }
}

/// Goldens on disk (for gated formats) whose example no longer exists.
fn stale_goldens(goldens: &Path, rows: &[GalleryRow]) -> Result<Vec<String>, String> {
    let mut stale = Vec::new();
    for fmt in FORMATS.iter().filter(|f| f.gated) {
        let dir = goldens.join(fmt.id);
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let path = entry
                .map_err(|e| format!("read {}: {e}", dir.display()))?
                .path();
            if path.extension().is_some_and(|e| e == "png") {
                let name = path.file_stem().unwrap().to_string_lossy().into_owned();
                if !rows.iter().any(|r| r.name == name) {
                    stale.push(format!("{}/{name}.png", fmt.id));
                }
            }
        }
    }
    stale.sort();
    Ok(stale)
}

fn summarize(rows: &[GalleryRow], stale: &[String], check: bool, accept: bool) -> (bool, String) {
    let mut failures: Vec<(String, GateFailure)> = Vec::new();
    let mut lines = String::new();
    let _ = writeln!(lines, "## Gridwell visual harness\n");
    let _ = writeln!(
        lines,
        "{} examples × {} formats\n",
        rows.len(),
        FORMATS.len()
    );
    let _ = writeln!(
        lines,
        "| format | ok | text | unavailable | error | gated failures |"
    );
    let _ = writeln!(
        lines,
        "|--------|----|------|-------------|-------|----------------|"
    );
    for fmt in FORMATS {
        let mut ok = 0;
        let mut text = 0;
        let mut unavail = 0;
        let mut err = 0;
        let mut gated_failures = 0;
        for row in rows {
            if let Some(cell) = row.cells.iter().find(|c| c.format_id == fmt.id) {
                match &cell.status {
                    CellStatus::Png { .. } => ok += 1,
                    CellStatus::Text { .. } => text += 1,
                    CellStatus::Unavailable(_) => unavail += 1,
                    CellStatus::Error(_) => err += 1,
                }
                if fmt.gated {
                    if let Some(f) = gate_failure(&cell.status) {
                        gated_failures += 1;
                        failures.push((format!("{}/{}", fmt.id, row.name), f));
                    }
                }
            }
        }
        let gated = if fmt.gated {
            gated_failures.to_string()
        } else {
            "—".into()
        };
        let _ = writeln!(
            lines,
            "| {} | {ok} | {text} | {unavail} | {err} | {gated} |",
            fmt.id
        );
    }

    if accept {
        let _ = writeln!(lines, "\n_Goldens updated for gated formats._");
        return (false, lines);
    }
    if !check {
        return (false, lines);
    }

    let failed = !failures.is_empty() || !stale.is_empty();
    if failed {
        let _ = writeln!(lines, "\n❌ Gated visual check failed.\n");
        for (cell, f) in &failures {
            let _ = writeln!(lines, "- `{cell}`: {}", f.label());
        }
        for path in stale {
            let _ = writeln!(
                lines,
                "- `{path}`: stale golden (no such example); delete it"
            );
        }
        if failures.iter().any(|(_, f)| {
            matches!(
                f,
                GateFailure::MissingGolden | GateFailure::Changed | GateFailure::SizeMismatch
            )
        }) {
            let _ = writeln!(
                lines,
                "\nIf the new rendering is correct, bless it: download the \
                 `proposed-goldens` artifact from this run and copy it over \
                 `harness/goldens/` (see harness/README.md)."
            );
        }
    } else {
        let _ = writeln!(lines, "\n✅ No gated visual regressions.");
    }
    (failed, lines)
}

fn write_gh_summary(summary: &str) {
    if let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY") {
        use std::io::Write as _;
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{summary}");
        }
    }
}

// ─────────────────────────────── html ────────────────────────────────────

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render_html(rows: &[GalleryRow], tools: &Tools) -> String {
    let mut h = String::new();
    h.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    h.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    h.push_str("<title>Gridwell render gallery</title>");
    h.push_str(STYLE);
    h.push_str("</head><body>");
    h.push_str("<h1>Gridwell render gallery</h1>");
    let _ = write!(
        h,
        "<p class=\"meta\">{} examples × {} formats · renderers: {}</p>",
        rows.len(),
        FORMATS.len(),
        esc(&tools.summary())
    );
    h.push_str(
        "<p class=\"legend\"><span class=\"b gated\">gated</span> formats block CI on change. \
        <span class=\"b unavail\">unavailable</span> = renderer not installed. \
        <span class=\"b err\">error</span> = render/raster failed. \
        <span class=\"b changed\">changed</span> / <span class=\"b new\">new</span> vs golden.</p>",
    );

    h.push_str("<table class=\"grid\"><thead><tr><th class=\"exh\">example</th>");
    for fmt in FORMATS {
        let cls = if fmt.gated { "gatedcol" } else { "" };
        let _ = write!(h, "<th class=\"{cls}\">{}</th>", fmt.id);
    }
    h.push_str("</tr></thead><tbody>");

    for row in rows {
        h.push_str("<tr><td class=\"exh\">");
        let _ = write!(
            h,
            "<div class=\"exname\">{}</div><div class=\"excat\">{}</div><div class=\"exdesc\">{}</div>",
            esc(row.name),
            esc(row.category),
            esc(row.description)
        );
        if !row.tags.is_empty() {
            h.push_str("<div class=\"tags\">");
            for t in &row.tags {
                let _ = write!(h, "<span class=\"tag\">{}</span>", esc(t));
            }
            h.push_str("</div>");
        }
        h.push_str("</td>");
        for cell in &row.cells {
            h.push_str("<td>");
            h.push_str(&cell_html(cell));
            h.push_str("</td>");
        }
        h.push_str("</tr>");
    }
    h.push_str("</tbody></table></body></html>");
    h
}

fn cell_html(cell: &Cell) -> String {
    match &cell.status {
        CellStatus::Png { rel, diff } => {
            let badge = match diff {
                Some(Comparison::Changed { fraction }) => {
                    format!(
                        "<span class=\"b changed\">changed {:.2}%</span>",
                        fraction * 100.0
                    )
                }
                Some(Comparison::SizeMismatch) => "<span class=\"b changed\">size ≠</span>".into(),
                Some(Comparison::New) => "<span class=\"b new\">new</span>".into(),
                _ => String::new(),
            };
            let link = cell.rel_out.as_deref().unwrap_or("#");
            format!(
                "<a href=\"{}\" target=\"_blank\" rel=\"noopener\"><img loading=\"lazy\" src=\"{}\"></a>{}",
                esc(link),
                esc(rel),
                badge
            )
        }
        CellStatus::Text { preview } => {
            let link = cell.rel_out.as_deref().unwrap_or("#");
            format!(
                "<pre class=\"src\">{}</pre><a class=\"raw\" href=\"{}\" target=\"_blank\" rel=\"noopener\">source</a>",
                esc(preview),
                esc(link)
            )
        }
        CellStatus::Unavailable(t) => {
            format!("<span class=\"b unavail\">no {}</span>", esc(t))
        }
        CellStatus::Error(e) => {
            format!("<span class=\"b err\" title=\"{}\">error</span>", esc(e))
        }
    }
}

const STYLE: &str = "<style>
* { box-sizing: border-box; }
body { margin: 0; padding: 24px; background: #f6f7f9; color: #1c2024;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; }
h1 { font-size: 22px; margin: 0 0 4px; letter-spacing: -0.01em; }
.meta { color: #5b636c; font-size: 13px; margin: 0 0 10px; }
.legend { color: #5b636c; font-size: 12px; margin: 0 0 20px; line-height: 1.9; }
.grid { border-collapse: separate; border-spacing: 0; background: #fff;
  border: 1px solid #e3e6ea; border-radius: 10px; overflow: hidden;
  box-shadow: 0 1px 3px rgba(0,0,0,.06); }
.grid th, .grid td { border-bottom: 1px solid #eceef1; border-right: 1px solid #eceef1;
  padding: 10px 12px; vertical-align: top; }
.grid tr:last-child td { border-bottom: none; }
.grid th:last-child, .grid td:last-child { border-right: none; }
.grid thead th { position: sticky; top: 0; z-index: 3; background: #eef1f4; color: #33393f;
  font-size: 11px; letter-spacing: .05em; text-transform: uppercase; font-weight: 700; text-align: left; }
th.gatedcol { background: #e3eeff; color: #1c4e8a; }
.exh { position: sticky; left: 0; background: #fbfcfd; z-index: 2; min-width: 190px; max-width: 220px; }
thead th.exh { z-index: 4; }
.exname { font-weight: 700; font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 13px; color: #14181c; word-break: break-word; }
.excat { color: #8a929b; font-size: 10px; letter-spacing: .06em; text-transform: uppercase; margin-top: 2px; }
.exdesc { font-size: 12px; color: #4a5159; margin: 5px 0; line-height: 1.4; }
.tags { margin-top: 5px; display: flex; flex-wrap: wrap; gap: 3px; }
.tag { font-size: 10px; background: #eef1f4; color: #48505a; border-radius: 999px; padding: 1px 8px; }
td a { display: inline-block; }
td a img { display: block; max-height: 210px; max-width: 300px; width: auto; height: auto;
  background: #fff; border: 1px solid #e3e6ea; border-radius: 6px; box-shadow: 0 1px 3px rgba(0,0,0,.08); }
.src { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 10px; line-height: 1.45;
  max-height: 200px; max-width: 300px; overflow: auto; background: #f6f7f9; color: #2b3138;
  border: 1px solid #e6e9ec; border-radius: 6px; padding: 8px; margin: 0 0 6px; white-space: pre; }
.raw { font-size: 11px; color: #2f6fd0; text-decoration: none; }
.raw:hover { text-decoration: underline; }
.b { display: inline-block; font-size: 10px; font-weight: 600; border-radius: 999px;
  padding: 2px 9px; margin-top: 6px; }
.gated { background: #e3eeff; color: #1c4e8a; }
.unavail { background: #eef1f4; color: #7c848d; }
.err { background: #fdece9; color: #b42318; }
.changed { background: #fff3e0; color: #a85800; }
.new { background: #e7f7ec; color: #1a7f37; }
</style>";

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &'static str, statuses: Vec<(&'static str, CellStatus)>) -> GalleryRow {
        GalleryRow {
            name,
            category: "structure",
            description: "",
            tags: vec![],
            cells: statuses
                .into_iter()
                .map(|(format_id, status)| Cell {
                    format_id,
                    rel_out: None,
                    status,
                })
                .collect(),
        }
    }

    fn png(diff: Option<Comparison>) -> CellStatus {
        CellStatus::Png {
            rel: String::new(),
            diff,
        }
    }

    fn gated_ids() -> Vec<&'static str> {
        FORMATS.iter().filter(|f| f.gated).map(|f| f.id).collect()
    }

    #[test]
    fn gated_formats_are_html_svg_typst() {
        assert_eq!(gated_ids(), vec!["html", "svg", "typst"]);
    }

    #[test]
    fn gate_failure_classifies_every_status() {
        assert_eq!(gate_failure(&png(Some(Comparison::Unchanged))), None);
        assert_eq!(gate_failure(&png(None)), None);
        assert_eq!(
            gate_failure(&png(Some(Comparison::New))),
            Some(GateFailure::MissingGolden)
        );
        assert_eq!(
            gate_failure(&png(Some(Comparison::Changed { fraction: 0.1 }))),
            Some(GateFailure::Changed)
        );
        assert_eq!(
            gate_failure(&png(Some(Comparison::SizeMismatch))),
            Some(GateFailure::SizeMismatch)
        );
        assert_eq!(
            gate_failure(&CellStatus::Unavailable("typst")),
            Some(GateFailure::NotRendered)
        );
        assert_eq!(
            gate_failure(&CellStatus::Error("boom".into())),
            Some(GateFailure::NotRendered)
        );
    }

    #[test]
    fn check_passes_only_when_every_gated_cell_matches() {
        let all_unchanged = row(
            "a",
            gated_ids()
                .into_iter()
                .map(|id| (id, png(Some(Comparison::Unchanged))))
                .collect(),
        );
        let (failed, summary) = summarize(&[all_unchanged], &[], true, false);
        assert!(!failed, "{summary}");
        assert!(summary.contains("✅"));
    }

    #[test]
    fn missing_golden_fails_check_and_explains_how_to_bless() {
        // Before this change a missing golden passed silently, so an empty goldens
        // directory made the gate a no-op.
        let r = row("a", vec![("html", png(Some(Comparison::New)))]);
        let (failed, summary) = summarize(&[r], &[], true, false);
        assert!(failed);
        assert!(summary.contains("`html/a`: missing golden"), "{summary}");
        assert!(summary.contains("proposed-goldens"), "{summary}");
    }

    #[test]
    fn unavailable_gated_renderer_fails_check() {
        let r = row("a", vec![("typst", CellStatus::Unavailable("typst"))]);
        let (failed, summary) = summarize(&[r], &[], true, false);
        assert!(failed);
        assert!(summary.contains("`typst/a`: not rendered"), "{summary}");
    }

    #[test]
    fn ungated_format_problems_never_fail_check() {
        let r = row(
            "a",
            vec![
                ("latex", CellStatus::Unavailable("xelatex")),
                ("docx", CellStatus::Error("libreoffice".into())),
            ],
        );
        let (failed, _) = summarize(&[r], &[], true, false);
        assert!(!failed);
    }

    #[test]
    fn stale_goldens_fail_check() {
        let (failed, summary) = summarize(&[], &["svg/gone.png".to_string()], true, false);
        assert!(failed);
        assert!(
            summary.contains("`svg/gone.png`: stale golden"),
            "{summary}"
        );
    }

    #[test]
    fn without_check_nothing_fails() {
        let r = row("a", vec![("html", png(Some(Comparison::New)))]);
        assert!(!summarize(&[r], &["svg/x.png".into()], false, false).0);
    }

    #[test]
    fn accept_never_fails() {
        let r = row("a", vec![("html", CellStatus::Error("x".into()))]);
        assert!(!summarize(&[r], &[], true, true).0);
    }

    #[test]
    fn stale_goldens_found_on_disk() {
        let dir = std::env::temp_dir().join(format!("gridwell-stale-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("svg")).unwrap();
        fs::write(dir.join("svg/kept.png"), b"").unwrap();
        fs::write(dir.join("svg/gone.png"), b"").unwrap();
        fs::write(dir.join("svg/notes.txt"), b"").unwrap();
        let rows = [row("kept", vec![])];
        assert_eq!(stale_goldens(&dir, &rows).unwrap(), vec!["svg/gone.png"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
