//! Grid read-back for the typeset formats: the cells Typst and TeX actually laid
//! out, not the cells our source says.
//!
//! **Typst** resolves every cell's position itself (auto-placement around
//! `colspan`/`rowspan`). A `show table.cell` rule records each resolved cell's
//! `x`, `y`, `colspan`, `rowspan` and plain text as `metadata`, and `typst query`
//! returns them — Typst's own grid, read back exactly, like html5ever's for HTML.
//!
//! **LaTeX** has no such introspection, so the table is compiled (LuaLaTeX, which
//! takes the corpus' Unicode) and read back from the PDF by position:
//!
//! - The test preamble redefines array's `\@acol` (the `\tabcolsep` space array
//!   puts on each side of every column, `\multicolumn`s included) to draw a tiny
//!   zero-width marker word in the middle of that space. Every TeX cell therefore
//!   shows two markers on its row's baseline, at its left and right edges. Nested
//!   tabulars (line breaks) use `@{}` and draw none. The markers add no width.
//! - `pdftotext -bbox` gives every word's box. Marker rows are the table's rows
//!   (even rows whose cells are all empty); marker pairs are its cells, and the
//!   distinct edge positions its column boundaries. A cell's columns follow from
//!   its edges, so a `\multicolumn` is recognised by its extent, even when its
//!   text is short and left-aligned. (Edges are matched to columns by the rows
//!   they occur in, not by x order: TeX gives a column that only spans cover zero
//!   or even negative width.)
//! - Each word goes to the row whose band contains its vertical centre (from 10pt
//!   above the row's baseline to 10pt above the next one: lines below the baseline
//!   in a cell, and raised footnote marks, stay in their row) and to the cell of
//!   that row whose edges contain its start.
//! - **Rowspans** (`\multirow`) are not part of TeX's alignment: the text is
//!   placed once, vertically centred over the rows, and each covered row has an
//!   empty cell of the span's width. So a merged area is verified as a whole: every
//!   row it covers must have a cell spanning exactly its columns, and the text of
//!   those cells together must be the expected text (see [`latex_compare`]).
//!   Where in the area the text sits is not checked: text left in the first row
//!   (no `\multirow`) passes too.
//! - Only the tabular itself is compiled (titles and notes are outside the grid).
//!   Text is compared on ASCII letters and digits only: LuaLaTeX's default fonts
//!   have no CJK or emoji glyphs, TeX turns quotes and dashes into typographic
//!   ones, and `p{}` columns hyphenate. Escaping is checked by the LaTeX crate's
//!   compile oracle; this one checks where text ends up.
//!
//! Many tables are typeset per document (one per page), in parallel batches.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use super::{look, norm, Expected, Grid, Want};

/// `true` if `tool` can be launched. Skips (with a note) when it can't, unless
/// the environment variable `require` is set, in which case this panics.
pub fn require(tool: &str, require: &str) -> bool {
    // Exit status ignored: tools disagree on version flags.
    if Command::new(tool).arg("-v").output().is_ok() {
        return true;
    }
    if std::env::var_os(require).is_some_and(|v| !v.is_empty()) {
        panic!("{tool} not found but {require} is set");
    }
    eprintln!("skipping: {tool} not found");
    false
}

/// A fresh directory, unique within the process (tests run in parallel).
fn work_dir(name: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("gridwell-grid-{}-{n}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// What reading a batch gives: one result per item, or `Err` if the batch as a
/// whole failed (did not compile).
type Batch<T> = Result<Vec<Result<T, String>>, String>;

/// Run `read` over `items` in up to one batch per CPU, `min` items or more per
/// batch, keeping the order; each batch gets its own work directory. A batch that
/// fails is bisected so the failure is pinned on the items that cause it (up to a
/// budget; past it, every item of a failing part reports the failure).
fn batched<T: Send>(
    name: &str,
    items: &[String],
    min: usize,
    read: impl Fn(&Path, &[String]) -> Batch<T> + Sync,
) -> Vec<Result<T, String>> {
    fn run<T>(
        name: &str,
        items: &[String],
        read: &dyn Fn(&Path, &[String]) -> Batch<T>,
        budget: &mut usize,
    ) -> Vec<Result<T, String>> {
        let dir = work_dir(name);
        let result = read(&dir, items);
        let _ = std::fs::remove_dir_all(&dir);
        match result {
            Ok(v) => {
                assert_eq!(v.len(), items.len(), "{name}: one result per item");
                v
            }
            Err(e) if items.len() == 1 => vec![Err(e)],
            Err(e) if *budget == 0 => items
                .iter()
                .map(|_| Err(format!("in a batch that failed:\n{e}")))
                .collect(),
            Err(_) => {
                *budget -= 1;
                let (a, b) = items.split_at(items.len() / 2);
                let mut out = run(&format!("{name}a"), a, read, budget);
                out.extend(run(&format!("{name}b"), b, read, budget));
                out
            }
        }
    }
    let cpus = std::thread::available_parallelism().map_or(4, |n| n.get());
    let per = items.len().div_ceil(cpus).max(min).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> = items
            .chunks(per)
            .enumerate()
            .map(|(i, chunk)| {
                let read = &read;
                s.spawn(move || run(&format!("{name}-{i}-"), chunk, read, &mut 40))
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    })
}

// ─── Typst ───

/// Plain text of cell content, and the metadata rule. Unknown elements show up
/// as `⟨name⟩` so a mismatch names them.
const TYPST_PRELUDE: &str = r#"#set page(width: 100cm, height: auto, margin: 1cm)
#let gw-plain(c) = {
  if c == none { "" }
  else if type(c) == str { c }
  else if repr(c.func()) in ("linebreak", "parbreak", "space") { " " }
  else if c.func() == smartquote { if c.double { "\"" } else { "'" } }
  else if c.has("text") { c.text }
  else if c.has("children") { c.children.map(gw-plain).fold("", (a, b) => a + b) }
  else if c.has("body") { gw-plain(c.body) }
  else if c.has("child") { gw-plain(c.child) }
  else { "⟨" + repr(c.func()) + "⟩" }
}
#show table.cell: it => [#metadata((x: it.x, y: it.y, cs: it.colspan, rs: it.rowspan, text: gw-plain(it.body))) <gw-cell>] + it
"#;

/// The grid Typst resolves for each table source (`Err`: it doesn't compile, or
/// cells overlap or leave holes).
pub fn typst_grids(sources: &[String]) -> Vec<Result<Grid, String>> {
    batched("typst", sources, 50, |dir, sources| {
        let mut doc = String::from(TYPST_PRELUDE);
        for src in sources {
            doc.push_str("#pagebreak(weak: true)\n#metadata(\"gw-table\") <gw-cell>\n");
            doc.push_str(src);
            doc.push('\n');
        }
        let file = dir.join("tables.typ");
        std::fs::write(&file, &doc).unwrap();
        let out = Command::new("typst")
            .arg("query")
            .arg(&file)
            .args(["<gw-cell>", "--field", "value"])
            .output()
            .unwrap();
        if !out.status.success() {
            return Err(format!(
                "typst query failed:\n{}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let values: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
        let mut tables: Vec<Vec<&Value>> = Vec::new();
        for v in &values {
            if v.as_str() == Some("gw-table") {
                tables.push(Vec::new());
            } else {
                tables
                    .last_mut()
                    .expect("cell before the first table")
                    .push(v);
            }
        }
        assert_eq!(tables.len(), sources.len());
        Ok(tables.into_iter().map(|cells| typst_grid(&cells)).collect())
    })
}

fn typst_grid(cells: &[&Value]) -> Result<Grid, String> {
    let num = |v: &Value, k: &str| v[k].as_u64().unwrap() as usize;
    let width = cells.iter().map(|c| num(c, "x") + num(c, "cs")).max();
    let height = cells.iter().map(|c| num(c, "y") + num(c, "rs")).max();
    let (Some(width), Some(height)) = (width, height) else {
        return Ok(Vec::new());
    };
    let mut grid: Vec<Vec<Option<String>>> = vec![vec![None; width]; height];
    for c in cells {
        let text = c["text"].as_str().unwrap_or_default();
        for row in &mut grid[num(c, "y")..num(c, "y") + num(c, "rs")] {
            for slot in &mut row[num(c, "x")..num(c, "x") + num(c, "cs")] {
                if slot.is_some() {
                    return Err(format!("cells overlap: {cells:?}"));
                }
                *slot = Some(text.to_string());
            }
        }
    }
    grid.into_iter()
        .map(|row| {
            row.into_iter()
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| format!("hole in the grid: {cells:?}"))
        })
        .collect()
}

// ─── LaTeX ───

/// `\tabcolsep` in the test documents (wider than the default 6pt, so markers
/// and text never touch), in PDF points: TeX's `pt` is 1/72.27in, PDF's 1/72in.
const TABCOLSEP: f64 = 12.0 * 72.0 / 72.27;

/// Cell edges closer than this are one column boundary. TeX places them exactly
/// (to about 0.001pt); distinct boundaries can be much closer than a glyph (a
/// column that only spans cover can be a fraction of a point wide).
const SAME_EDGE: f64 = 0.1;

/// What the test preamble needs: the writer's packages, a page big enough for any
/// table, and the cell-edge markers (see the module docs).
const LATEX_PREAMBLE: &str = r"\documentclass{article}
\usepackage[paperwidth=100in,paperheight=100in,margin=1in]{geometry}
\usepackage{booktabs,multirow,longtable}
\usepackage[normalem]{ulem}
\usepackage[table]{xcolor}
\setlength{\parindent}{0pt}
\pagestyle{empty}
\setlength{\tabcolsep}{12pt}
\setcounter{LTchunksize}{100000}
\makeatletter
\protected\def\gwmark{\makebox[0pt]{\fontsize{2}{2}\selectfont GWQ}}
\def\@acol{\@addtopreamble{\hskip.5\col@sep\gwmark\hskip.5\col@sep}}
\makeatother
\begin{document}
";

const MARKER: &str = "GWQ";

/// One TeX cell: the column boundaries at its edges (indices into
/// [`TexTable::boundaries`]) and its text.
#[derive(Debug, Clone)]
pub struct TexCell {
    pub from: usize,
    pub to: usize,
    pub text: String,
}

/// A table as TeX laid it out: its column boundaries (distinct cell-edge
/// positions, in x order) and rows of cells, left to right.
///
/// Boundaries are identified by position but not *ordered* by it. A column that
/// only spanned cells occupy gets no width of its own in TeX's alignment: it can
/// come out with zero width (two boundaries at one position) or even a negative
/// one (its right boundary left of its left one). Which column boundary a
/// position is follows from the rows it appears in (see [`latex_compare`]).
#[derive(Debug, Clone, Default)]
pub struct TexTable {
    pub boundaries: Vec<f64>,
    pub rows: Vec<Vec<TexCell>>,
}

/// The tabular (or longtable) of the writer's output: titles and notes are
/// outside the grid. `None` when there is none (every column hidden).
pub fn latex_tabular(src: &str) -> Option<&str> {
    let start = src
        .find("\\begin{tabular}{")
        .into_iter()
        .chain(src.find("\\begin{longtable}{"))
        .min()?;
    let end = ["\n\\end{tabular}\n", "\n\\end{longtable}\n"]
        .iter()
        .filter_map(|e| src[start..].find(e).map(|i| start + i + e.len()))
        .min()?;
    Some(&src[start..end])
}

/// Typeset each tabular and read it back (`Err`: it doesn't compile, or its
/// cells can't be made out).
pub fn latex_tables(tabulars: &[String]) -> Vec<Result<TexTable, String>> {
    // On a fresh machine LuaLaTeX builds its font cache on first use; do that once
    // before parallel batches would race to.
    static WARM: std::sync::Once = std::sync::Once::new();
    WARM.call_once(|| {
        let dir = work_dir("latex-warm");
        let doc = format!("{LATEX_PREAMBLE}x\\end{{document}}\n");
        let _ = compile(&dir, &doc);
        let _ = std::fs::remove_dir_all(&dir);
    });
    batched("latex", tabulars, 40, |dir, tabulars| {
        let mut doc = String::from(LATEX_PREAMBLE);
        for t in tabulars {
            doc.push_str(t);
            doc.push_str("\\clearpage\n");
        }
        doc.push_str("\\end{document}\n");
        let pages = compile(dir, &doc)?;
        assert_eq!(pages.len(), tabulars.len(), "one page per table");
        Ok(pages.iter().map(|words| tex_table(words)).collect())
    })
}

/// Compile with LuaLaTeX (again while longtable asks for it: its column widths
/// settle over runs) and return each page's words.
fn compile(dir: &Path, doc: &str) -> Result<Vec<Vec<Word>>, String> {
    std::fs::write(dir.join("tables.tex"), doc).unwrap();
    for _ in 0..3 {
        let out = Command::new("lualatex")
            .args(["-interaction=nonstopmode", "-halt-on-error", "tables.tex"])
            .current_dir(dir)
            .output()
            .unwrap();
        let log = std::fs::read_to_string(dir.join("tables.log")).unwrap_or_default();
        if !out.status.success() {
            let err: Vec<&str> = log
                .lines()
                .skip_while(|l| !l.starts_with('!'))
                .take(10)
                .collect();
            return Err(format!("lualatex failed:\n{}", err.join("\n")));
        }
        if !log.contains("Rerun LaTeX") {
            break;
        }
    }
    let out = Command::new("pdftotext")
        .args(["-bbox", "tables.pdf", "-"])
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "pdftotext failed");
    Ok(parse_bbox(&String::from_utf8_lossy(&out.stdout)))
}

#[derive(Debug, Clone)]
struct Word {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    text: String,
}

impl Word {
    fn cx(&self) -> f64 {
        (self.x0 + self.x1) / 2.0
    }
    fn cy(&self) -> f64 {
        (self.y0 + self.y1) / 2.0
    }
    fn is_marker(&self) -> bool {
        self.text == MARKER && self.y1 - self.y0 < 4.0
    }
}

/// The words of `pdftotext -bbox` output, page by page.
fn parse_bbox(xhtml: &str) -> Vec<Vec<Word>> {
    let attr = |line: &str, name: &str| -> f64 {
        let key = format!("{name}=\"");
        let i = line.find(&key).unwrap() + key.len();
        line[i..i + line[i..].find('"').unwrap()].parse().unwrap()
    };
    let mut pages: Vec<Vec<Word>> = Vec::new();
    for line in xhtml.lines().map(str::trim) {
        if line.starts_with("<page ") {
            pages.push(Vec::new());
        } else if line.starts_with("<word ") {
            let text = &line[line.find('>').unwrap() + 1..line.rfind("</word>").unwrap()];
            let text = text
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&apos;", "'")
                .replace("&amp;", "&");
            pages.last_mut().unwrap().push(Word {
                x0: attr(line, "xMin"),
                y0: attr(line, "yMin"),
                x1: attr(line, "xMax"),
                y1: attr(line, "yMax"),
                text,
            });
        }
    }
    pages
}

/// Sorted distinct values, merging values closer than `tol`.
fn distinct(mut v: Vec<f64>, tol: f64) -> Vec<f64> {
    v.sort_by(f64::total_cmp);
    v.dedup_by(|b, a| *b - *a < tol);
    v
}

fn index_of(values: &[f64], x: f64) -> usize {
    values
        .iter()
        .position(|v| (v - x).abs() < SAME_EDGE)
        .unwrap()
}

/// Rebuild one page's table from its markers and words.
fn tex_table(words: &[Word]) -> Result<TexTable, String> {
    let (markers, words): (Vec<&Word>, Vec<&Word>) = words.iter().partition(|w| w.is_marker());
    if markers.is_empty() {
        return match words.first() {
            None => Ok(TexTable::default()),
            Some(_) => Err(format!("text but no cells: {words:?}")),
        };
    }
    // Rows: markers sharing a baseline. Cells: consecutive marker pairs.
    let ys = distinct(markers.iter().map(|m| m.cy()).collect(), 2.0);
    let mut edges: Vec<Vec<(f64, f64)>> = Vec::new();
    for &y in &ys {
        let mut xs: Vec<f64> = markers
            .iter()
            .filter(|m| (m.cy() - y).abs() < 2.0)
            .map(|m| m.cx())
            .collect();
        xs.sort_by(f64::total_cmp);
        if xs.len() % 2 != 0 {
            return Err(format!("odd number of cell markers in a row: {xs:?}"));
        }
        edges.push(xs.chunks(2).map(|p| (p[0], p[1])).collect());
    }
    let half = TABCOLSEP / 2.0;
    let boundaries = distinct(
        edges
            .iter()
            .flatten()
            .flat_map(|&(l, r)| [l - half, r + half])
            .collect(),
        SAME_EDGE,
    );
    let id = |x: f64| index_of(&boundaries, x);
    /// A cell being filled: its edges (x) and the words found in it.
    struct Filling<'w> {
        cell: TexCell,
        left: f64,
        right: f64,
        words: Vec<&'w Word>,
    }
    let mut rows: Vec<Vec<Filling>> = edges
        .iter()
        .map(|row| {
            row.iter()
                .map(|&(l, r)| {
                    let cell = TexCell {
                        from: id(l - half),
                        to: id(r + half),
                        text: String::new(),
                    };
                    Filling {
                        cell,
                        left: l - half,
                        right: r + half,
                        words: Vec::new(),
                    }
                })
                .collect()
        })
        .collect();
    // Marker centres sit about 0.85pt above the baseline.
    let baselines: Vec<f64> = ys.iter().map(|y| y + 0.85).collect();
    for w in words {
        let r = baselines
            .iter()
            .rposition(|b| b - 10.0 <= w.cy())
            .unwrap_or(0);
        let x = w.x0 + 0.5;
        let Some(cell) = rows[r].iter_mut().find(|c| c.left <= x && x < c.right) else {
            return Err(format!("{:?} at x={x:.1} is in no cell of row {r}", w.text));
        };
        cell.words.push(w);
    }
    let rows = rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(
                    |Filling {
                         mut cell,
                         mut words,
                         ..
                     }| {
                        // Reading order: lines (raised marks share their line), then x.
                        words.sort_by(|a, b| a.cy().total_cmp(&b.cy()));
                        let mut lines: Vec<Vec<&Word>> = Vec::new();
                        for w in words {
                            match lines.last_mut() {
                                Some(line) if w.cy() - line[0].cy() < 5.0 => line.push(w),
                                _ => lines.push(vec![w]),
                            }
                        }
                        for mut line in lines {
                            line.sort_by(|a, b| a.x0.total_cmp(&b.x0));
                            for w in line {
                                cell.text.push_str(&w.text);
                                cell.text.push(' ');
                            }
                        }
                        cell
                    },
                )
                .collect()
        })
        .collect();
    Ok(TexTable { boundaries, rows })
}

/// Text as compared for LaTeX: ASCII letters and digits (see the module docs).
/// A letter followed by combining marks is dropped with them, as its precomposed
/// form (which the PDF may hold instead) is.
pub fn latex_norm(s: &str) -> String {
    let combining = |c: char| {
        matches!(c, '\u{300}'..='\u{36F}' | '\u{1AB0}'..='\u{1AFF}' | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}' | '\u{FE20}'..='\u{FE2F}')
    };
    let chars: Vec<char> = s.chars().collect();
    chars
        .iter()
        .enumerate()
        .filter(|&(i, c)| {
            c.is_ascii_alphanumeric() && !chars.get(i + 1).is_some_and(|&n| combining(n))
        })
        .map(|(_, c)| *c)
        .collect()
}

/// Whether every column boundary of `want` is visible in TeX's layout: some row
/// has a cell starting there. A boundary every row spans across has no edge to
/// see (as with RTF's `\cellx`), so such tables can't be checked.
pub fn latex_observable(want: &Expected) -> bool {
    (1..want.width()).all(|c| want.regions.iter().any(|r| r.cols.start == c))
}

/// Compare a TeX table with the expected grid.
///
/// First the structure: each row's cell edges, left to right, must be the
/// expected row's region edges, and each expected column boundary must always be
/// at the same position. That pins every edge to a column without relying on x
/// order (see [`TexTable`]). One position may stand for several column
/// boundaries (a zero-width column), never within one row: every TeX cell has
/// width.
///
/// Then merged area by merged area: each row a region covers has a cell over
/// exactly the region's columns, and the text of those cells, top to bottom, must
/// be the region's text.
pub fn latex_compare(got: &TexTable, want: &Expected) -> Result<(), String> {
    let context = || format!("\n got: {got:?}\nwant: {:?}", want.rows);
    if got.rows.len() != want.rows.len() {
        return Err(format!(
            "{} rows, expected {}{}",
            got.rows.len(),
            want.rows.len(),
            context()
        ));
    }
    // Column boundary → the TeX boundary it is at.
    let mut boundary = vec![None; want.width() + 1];
    let mut cells: Vec<Vec<(Range<usize>, &str)>> = Vec::new();
    for (r, row) in got.rows.iter().enumerate() {
        let mut edges: Vec<usize> = want
            .regions
            .iter()
            .filter(|g| g.rows.contains(&r))
            .map(|g| g.cols.start)
            .collect();
        edges.sort_unstable();
        edges.push(want.width());
        let got_edges: Vec<usize> = row
            .iter()
            .map(|c| c.from)
            .chain(row.last().map(|c| c.to))
            .collect();
        let joined = row.windows(2).all(|w| w[0].to == w[1].from);
        let wide = row.iter().all(|c| c.from != c.to);
        if !joined || !wide || got_edges.len() != edges.len() {
            return Err(format!(
                "row {r}: {} cells, expected {} (edges {got_edges:?} → columns {edges:?}){}",
                row.len(),
                edges.len() - 1,
                context()
            ));
        }
        for (&b, &c) in got_edges.iter().zip(&edges) {
            if *boundary[c].get_or_insert(b) != b {
                return Err(format!(
                    "row {r}: column boundary {c} is at x={:.1} here but at x={:.1} elsewhere{}",
                    got.boundaries[b],
                    got.boundaries[boundary[c].unwrap()],
                    context()
                ));
            }
        }
        cells.push(
            row.iter()
                .zip(edges.windows(2))
                .map(|(c, e)| (e[0]..e[1], c.text.as_str()))
                .collect(),
        );
    }
    for region in &want.regions {
        let mut text = String::new();
        for r in region.rows.clone() {
            let Some((_, t)) = cells[r].iter().find(|(cols, _)| *cols == region.cols) else {
                return Err(format!(
                    "row {r}: no cell over columns {:?} (cells: {:?}){}",
                    region.cols,
                    cells[r].iter().map(|c| &c.0).collect::<Vec<_>>(),
                    context()
                ));
            };
            text.push_str(t);
        }
        let ok = match &want.rows[region.rows.start][region.cols.start] {
            Want::Any => true,
            Want::Text(t) => latex_norm(&norm(&text)) == latex_norm(t),
        };
        if !ok {
            return Err(format!(
                "rows {:?} × columns {:?}: got {text:?}, want {:?}{}",
                region.rows,
                region.cols,
                want.rows[region.rows.start][region.cols.start],
                context()
            ));
        }
    }
    Ok(())
}

// ─── Typst styles ───

/// Records every cell (Typst's resolved `fill`, `align`, `stroke`) and every
/// text run with its resolved style (`context text.*`). Underline, strike,
/// strong, emph and super aren't text properties, so nesting counters track them.
const TYPST_LOOK_PRELUDE: &str = r#"#set page(width: 100cm, height: auto, margin: 1cm)
#let gw-u = state("gw-u", 0)
#let gw-s = state("gw-s", 0)
#let gw-b = state("gw-b", 0)
#let gw-e = state("gw-e", 0)
#let gw-p = state("gw-p", 0)
#let gw-wrap(st, it) = { st.update(x => x + 1); it; st.update(x => x - 1) }
#show underline: it => gw-wrap(gw-u, it)
#show strike: it => gw-wrap(gw-s, it)
#show strong: it => gw-wrap(gw-b, it)
#show emph: it => gw-wrap(gw-e, it)
#show super: it => gw-wrap(gw-p, it)
#let gw-hex(c) = if type(c) == color { c.to-hex() } else { none }
#let gw-side(s) = if type(s) == stroke {
  (t: if type(s.thickness) == length { s.thickness.to-absolute().pt() } else { none },
   paint: gw-hex(s.paint), dash: repr(s.dash))
} else { none }
#let gw-strokes(s) = if type(s) == dictionary {
  (top: gw-side(s.at("top", default: none)), right: gw-side(s.at("right", default: none)),
   bottom: gw-side(s.at("bottom", default: none)), left: gw-side(s.at("left", default: none)))
} else if type(s) == stroke {
  let one = gw-side(s); (top: one, right: one, bottom: one, left: one)
} else { none }
#show table.cell: it => {
  [#metadata((k: "cell", fill: gw-hex(it.fill), align: repr(it.align), stroke: gw-strokes(it.stroke))) <gw>]
  it
  [#metadata((k: "end")) <gw>]
}
#show text: it => context [#metadata((k: "run", t: it.text, w: repr(text.weight), st: repr(text.style),
  fill: gw-hex(text.fill), size: text.size.to-absolute().pt(),
  font: if type(text.font) == array { text.font.at(0) } else { text.font },
  u: gw-u.get(), s: gw-s.get(), b: gw-b.get(), e: gw-e.get(), p: gw-p.get())) <gw>] + it
"#;

/// A Unicode superscript character's plain form.
fn unsuper(c: char) -> char {
    match c {
        '⁰' => '0',
        '¹' => '1',
        '²' => '2',
        '³' => '3',
        '⁴' => '4',
        '⁵' => '5',
        '⁶' => '6',
        '⁷' => '7',
        '⁸' => '8',
        '⁹' => '9',
        '⁺' => '+',
        '⁻' => '-',
        '⁼' => '=',
        '⁽' => '(',
        '⁾' => ')',
        'ⁱ' => 'i',
        'ⁿ' => 'n',
        c => c,
    }
}

fn typst_align(repr: &str) -> (Option<look::Align>, Option<look::VAlign>) {
    let mut h = None;
    let mut v = None;
    for part in repr.split('+').map(str::trim) {
        match part {
            "left" | "start" => h = Some(look::Align::Left),
            "center" => h = Some(look::Align::Center),
            "right" | "end" => h = Some(look::Align::Right),
            "top" => v = Some(look::VAlign::Top),
            "horizon" => v = Some(look::VAlign::Middle),
            "bottom" => v = Some(look::VAlign::Bottom),
            _ => {}
        }
    }
    (h, v)
}

fn typst_edge(v: &Value) -> Option<look::Edge> {
    if v.is_null() {
        return None;
    }
    let dash = v["dash"].as_str().unwrap_or("none");
    Some(look::Edge {
        line: if dash.contains("dot") {
            look::Line::Dotted
        } else if dash == "none" {
            look::Line::Solid
        } else {
            look::Line::Dashed
        },
        width_pt: v["t"].as_f64(),
        color: v["paint"].as_str().and_then(look::hex),
    })
}

/// How each Typst source's table cells look, as Typst resolves them. `Err` if
/// the source doesn't compile.
pub fn typst_looks(sources: &[String]) -> Vec<Result<look::Doc, String>> {
    batched("typst-look", sources, 50, |dir, sources| {
        let mut doc = String::from(TYPST_LOOK_PRELUDE);
        for src in sources {
            doc.push_str("#pagebreak(weak: true)\n#metadata((k: \"table\")) <gw>\n");
            doc.push_str(src);
            doc.push('\n');
        }
        let file = dir.join("tables.typ");
        std::fs::write(&file, &doc).unwrap();
        let out = Command::new("typst")
            .arg("query")
            .arg(&file)
            .args(["<gw>", "--field", "value"])
            .output()
            .unwrap();
        if !out.status.success() {
            return Err(format!(
                "typst query failed:\n{}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let values: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
        let mut docs: Vec<look::Doc> = Vec::new();
        let mut cell: Option<look::Look> = None;
        for v in &values {
            match v["k"].as_str() {
                Some("table") => docs.push(look::Doc::default()),
                Some("cell") => {
                    let (align, valign) = typst_align(v["align"].as_str().unwrap_or(""));
                    let s = &v["stroke"];
                    cell = Some(look::Look {
                        fill: v["fill"].as_str().and_then(look::hex),
                        align,
                        valign,
                        borders: ["top", "right", "bottom", "left"].map(|k| typst_edge(&s[k])),
                        ..look::Look::default()
                    });
                }
                Some("end") => {
                    if let Some(c) = cell.take() {
                        docs.last_mut().unwrap().cells.push(look::finish(c));
                    }
                }
                Some("run") => {
                    let weight = v["w"].as_str().unwrap_or("").trim_matches('"').to_string();
                    let n = |k: &str| v[k].as_i64().unwrap_or(0);
                    let bold =
                        matches!(weight.as_str(), "bold" | "semibold" | "extrabold" | "black")
                            || weight.parse::<u32>().is_ok_and(|w| w >= 600)
                            || n("b") > 0;
                    let italic = (v["st"].as_str().unwrap_or("").contains("italic")
                        || v["st"].as_str().unwrap_or("").contains("oblique"))
                        ^ (n("e") % 2 == 1);
                    let color = v["fill"]
                        .as_str()
                        .and_then(look::hex)
                        .filter(|c| *c != (0, 0, 0));
                    let base = look::Run {
                        text: String::new(),
                        bold,
                        italic,
                        underline: n("u") > 0,
                        strike: n("s") > 0,
                        superscript: n("p") > 0,
                        color,
                        size_pt: v["size"].as_f64(),
                        family: v["font"].as_str().map(str::to_string),
                    };
                    // Typst ≤ 0.12 renders `super` with Unicode superscript
                    // characters where it can (`#super[7]` → "⁷"), in the same text
                    // element as what precedes it; newer versions use the font's
                    // superscript glyphs and keep the text. Split such characters
                    // into superscript runs of their plain form.
                    let raw = v["t"].as_str().unwrap_or("");
                    let mut parts: Vec<look::Run> = Vec::new();
                    for ch in raw.chars() {
                        let plain = unsuper(ch);
                        let sup = base.superscript || plain != ch;
                        match parts.last_mut() {
                            Some(r) if r.superscript == sup => r.text.push(plain),
                            _ => parts.push(look::Run {
                                text: plain.to_string(),
                                superscript: sup,
                                ..base.clone()
                            }),
                        }
                    }
                    match cell.as_mut() {
                        Some(c) => {
                            if !c.runs.is_empty() {
                                // Typst splits words into separate text elements.
                                look::push_run(
                                    &mut c.runs,
                                    look::Run {
                                        text: " ".into(),
                                        ..base.clone()
                                    },
                                );
                            }
                            for r in parts {
                                look::push_run(&mut c.runs, r);
                            }
                        }
                        None => {
                            let d = docs.last_mut().unwrap();
                            d.outside.extend(parts.iter().map(|r| r.text.as_str()));
                            d.outside.push(' ');
                        }
                    }
                }
                _ => {}
            }
        }
        assert_eq!(docs.len(), sources.len());
        Ok(docs.into_iter().map(Ok).collect())
    })
}
