//! Conformance: whether an emitter's IR says what an expected table says.
//!
//! Two correct emitters can write the same table very differently: other style
//! ids, a composition instead of a definition, a color as `red` or `#FF0000`,
//! keys in another order. So tables are compared as **normalized grids**:
//! [`normalize`] resolves the IR (as every writer does) and writes down only
//! what a reader sees, with every style a set of resolved properties, every piece
//! of content its text and runs, and footnote marks by footnote number rather
//! than id.
//!
//! [`check`] compares a table's grid with an expected one, in tiers:
//!
//! | Tier | Checks |
//! |---|---|
//! | L0 valid | the IR parses, and validation finds no errors |
//! | L1 structure | columns and stub; header lines; head rows and spans; groups and their labels; row roles; data and summary rows, and summary rows at the top; cell text |
//! | L2 content | runs (inline styles, line breaks, images, raw content), footnote marks (placement, order, `mark_text`) and each footnote's mark, typed values, `scope`, column and group ids, `lang` and `dir` |
//! | L3 styling | each cell's, line's and run's resolved style; alignment; column, table and image sizes |
//!
//! Each tier includes the ones below it: a table passes L2 if nothing differs at
//! L0, L1 or L2. A grid can be written to a tier ([`normalize`] with `L1` leaves
//! out content and styling), and an expected grid is only checked to the tier it
//! goes to.
//!
//! A grid is JSON:
//!
//! ```json
//! {"format": "gridwell-conformance-grid/1", "tier": "L3", "grid": {…}}
//! ```
//!
//! Other top-level keys (an expected grid's `case`, `producer` and `source`, say)
//! are kept for people and ignored by [`check`].

use std::fmt;
use std::str::FromStr;

use gridwell_core::Length;
use gridwell_ir::content::ContentNode;
use gridwell_ir::{FontWeight, Keyword, Table, ValidatedTable};
use gridwell_layout::{
    resolve_with, Line, ResolveOptions, ResolvedCell, ResolvedRow, ResolvedStyle, ResolvedTable,
};
use serde_json::{json, Map, Value};
use thiserror::Error;

/// The `format` of a normalized grid. The number changes when a grid's meaning
/// does, so old expected grids are refused rather than misread.
pub const FORMAT: &str = "gridwell-conformance-grid/1";

/// How many differences [`Report`]'s text lists before saying how many more.
const SHOWN: usize = 20;

/// A conformance tier. Each includes the ones before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// The IR parses and validates.
    L0,
    /// Structure and text.
    L1,
    /// Content: runs, marks, typed values.
    L2,
    /// Resolved styling.
    L3,
}

impl Tier {
    pub const ALL: [Tier; 4] = [Tier::L0, Tier::L1, Tier::L2, Tier::L3];

    pub fn as_str(self) -> &'static str {
        match self {
            Tier::L0 => "L0",
            Tier::L1 => "L1",
            Tier::L2 => "L2",
            Tier::L3 => "L3",
        }
    }

    /// What the tier checks, in a word or two.
    pub fn title(self) -> &'static str {
        match self {
            Tier::L0 => "valid",
            Tier::L1 => "structure",
            Tier::L2 => "content",
            Tier::L3 => "styling",
        }
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Tier {
    type Err = ConformanceError;

    /// `L0` to `L3`, either case.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Tier::ALL
            .into_iter()
            .find(|t| t.as_str().eq_ignore_ascii_case(s))
            .ok_or_else(|| ConformanceError::UnknownTier(s.to_string()))
    }
}

/// Why a check couldn't be made at all (as opposed to a table that fails one).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConformanceError {
    #[error("unknown tier \"{0}\"; tiers are L0, L1, L2 and L3")]
    UnknownTier(String),
    /// The expected grid isn't one: not JSON, not an object, the wrong `format`,
    /// no `tier` or `grid`.
    #[error("the expected grid {0}")]
    NotAGrid(String),
    /// A check to a tier the expected grid doesn't go to.
    #[error("the expected grid goes to {specified}, so it can't be checked to {requested}")]
    TierNotSpecified { requested: Tier, specified: Tier },
}

/// The normalized grid of a table, written to `tier` (see the module docs).
pub fn normalize(table: &ValidatedTable<'_>, tier: Tier) -> Value {
    let mut grid = grid(table);
    project(&mut grid, Tier::L0, tier);
    json!({"format": FORMAT, "tier": tier.as_str(), "grid": grid})
}

/// Check a table against an expected grid, to `tier` (by default, the tier the
/// expected grid goes to).
///
/// An invalid table fails L0; its grid isn't compared.
pub fn check(
    table: &Table,
    expected: &Value,
    tier: Option<Tier>,
) -> Result<Report, ConformanceError> {
    let (specified, expected_grid) = expected_grid(expected)?;
    let tier = tier.unwrap_or(specified);
    if tier > specified {
        return Err(ConformanceError::TierNotSpecified {
            requested: tier,
            specified,
        });
    }
    let validated = match table.validated() {
        Ok(v) => v,
        Err(e) => {
            return Ok(Report {
                tier,
                errors: e.errors.iter().map(ToString::to_string).collect(),
                differences: Vec::new(),
            })
        }
    };
    let mut actual = grid(&validated);
    project(&mut actual, Tier::L0, tier);
    let mut expected_grid = expected_grid.clone();
    project(&mut expected_grid, Tier::L0, tier);
    let mut differences = Vec::new();
    compare(
        &mut String::new(),
        Tier::L0,
        Some(&expected_grid),
        Some(&actual),
        &mut differences,
    );
    Ok(Report {
        tier,
        errors: Vec::new(),
        differences,
    })
}

/// [`check`] for IR as JSON text. IR that doesn't parse fails L0.
pub fn check_json(
    ir: &str,
    expected: &str,
    tier: Option<Tier>,
) -> Result<Report, ConformanceError> {
    let expected: Value = serde_json::from_str(expected)
        .map_err(|e| ConformanceError::NotAGrid(format!("isn't JSON: {e}")))?;
    match Table::from_json(ir) {
        Ok(table) => check(&table, &expected, tier),
        Err(e) => {
            let (specified, _) = expected_grid(&expected)?;
            let tier = tier.unwrap_or(specified);
            if tier > specified {
                return Err(ConformanceError::TierNotSpecified {
                    requested: tier,
                    specified,
                });
            }
            Ok(Report {
                tier,
                errors: vec![format!("the IR doesn't parse: {e}")],
                differences: Vec::new(),
            })
        }
    }
}

/// The tier and grid of an expected grid.
fn expected_grid(expected: &Value) -> Result<(Tier, &Value), ConformanceError> {
    let not = |why: String| ConformanceError::NotAGrid(why);
    let obj = expected
        .as_object()
        .ok_or_else(|| not("isn't a JSON object".into()))?;
    match obj.get("format") {
        Some(Value::String(f)) if f == FORMAT => {}
        Some(f) => return Err(not(format!("has format {f}, not \"{FORMAT}\""))),
        None => return Err(not(format!("has no format (\"{FORMAT}\")"))),
    }
    let tier = match obj.get("tier") {
        Some(Value::String(t)) => t
            .parse()
            .map_err(|_| not(format!("has an unknown tier \"{t}\"")))?,
        _ => return Err(not("has no tier".into())),
    };
    match obj.get("grid") {
        Some(g @ Value::Object(_)) => Ok((tier, g)),
        _ => Err(not("has no grid object".into())),
    }
}

/// How a tier went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
    /// Above the tier checked.
    NotChecked,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Fail => "fail",
            Status::NotChecked => "not checked",
        }
    }
}

/// One thing that differs between the expected grid and the table's.
#[derive(Debug, Clone, PartialEq)]
pub struct Difference {
    /// The tier it fails.
    pub tier: Tier,
    /// Where, as `groups[0].rows[2].cells[1].text`.
    pub path: String,
    /// `None`: not in the expected grid.
    pub expected: Option<Value>,
    /// `None`: not in the table's grid.
    pub actual: Option<Value>,
}

impl fmt::Display for Difference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let show = |v: &Option<Value>| match v {
            Some(v) => clip(&v.to_string(), 120),
            None => "nothing".to_string(),
        };
        write!(
            f,
            "{} {}: expected {}, got {}",
            self.tier,
            self.path,
            show(&self.expected),
            show(&self.actual)
        )
    }
}

fn clip(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// The result of a [`check`].
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The tier checked to.
    pub tier: Tier,
    /// Why the IR fails L0 (it doesn't parse, or validation errors); empty when it
    /// passes.
    pub errors: Vec<String>,
    /// Every difference, in grid order; empty when the IR fails L0.
    pub differences: Vec<Difference>,
}

impl Report {
    /// How `tier` went: it passes when it and every tier below it do.
    pub fn status(&self, tier: Tier) -> Status {
        if tier > self.tier {
            Status::NotChecked
        } else if !self.errors.is_empty() || self.differences.iter().any(|d| d.tier <= tier) {
            Status::Fail
        } else {
            Status::Pass
        }
    }

    /// Whether the table passes the tier checked.
    pub fn passed(&self) -> bool {
        self.status(self.tier) == Status::Pass
    }

    /// The report as JSON, for the bindings: `passed`, `tier`, each tier's status
    /// (`tiers`), `errors`, `differences` (each with `tier`, `path`, and `expected`
    /// and `actual` when there) and the report as `text`.
    pub fn to_json(&self) -> Value {
        let tiers: Map<String, Value> = Tier::ALL
            .into_iter()
            .map(|t| (t.as_str().to_string(), json!(self.status(t).as_str())))
            .collect();
        let differences: Vec<Value> = self
            .differences
            .iter()
            .map(|d| {
                // Absent (not in that grid) is no key; `null` is a null there.
                let mut m = Map::new();
                m.insert("tier".into(), json!(d.tier.as_str()));
                m.insert("path".into(), json!(d.path));
                for (k, v) in [("expected", &d.expected), ("actual", &d.actual)] {
                    if let Some(v) = v {
                        m.insert(k.into(), v.clone());
                    }
                }
                Value::Object(m)
            })
            .collect();
        json!({
            "passed": self.passed(),
            "tier": self.tier.as_str(),
            "tiers": tiers,
            "errors": self.errors,
            "differences": differences,
            "text": self.to_string(),
        })
    }
}

impl fmt::Display for Report {
    /// ```text
    /// fails L1 (checked to L3)
    ///   L0 valid: pass
    ///   L1 structure: fail (2 differences)
    ///   …
    /// 2 differences:
    ///   L1 groups[0].rows[0].cells[1].text: expected "1.111e-01", got "0.1111"
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match Tier::ALL
            .into_iter()
            .find(|&t| self.status(t) == Status::Fail)
        {
            Some(t) => writeln!(f, "fails {t} (checked to {})", self.tier)?,
            None => writeln!(f, "passes {}", self.tier)?,
        }
        for t in Tier::ALL {
            write!(f, "  {t} {}: {}", t.title(), self.status(t).as_str())?;
            let n = self.differences.iter().filter(|d| d.tier == t).count();
            match (t, n) {
                (Tier::L0, _) if !self.errors.is_empty() => {
                    write!(f, " ({})", plural(self.errors.len(), "error"))?
                }
                (_, 0) => {}
                _ => write!(f, " ({})", plural(n, "difference"))?,
            }
            writeln!(f)?;
        }
        if !self.errors.is_empty() {
            writeln!(f, "{}:", plural(self.errors.len(), "error"))?;
            for e in &self.errors {
                writeln!(f, "  {e}")?;
            }
        }
        if !self.differences.is_empty() {
            writeln!(f, "{}:", plural(self.differences.len(), "difference"))?;
            for d in self.differences.iter().take(SHOWN) {
                writeln!(f, "  {d}")?;
            }
            if self.differences.len() > SHOWN {
                writeln!(f, "  … and {} more", self.differences.len() - SHOWN)?;
            }
        }
        Ok(())
    }
}

fn plural(n: usize, what: &str) -> String {
    if n == 1 {
        format!("1 {what}")
    } else {
        format!("{n} {what}s")
    }
}

// ─── Comparing ───

/// The tier of a key, by name. Inside a key, nothing is checked below its tier:
/// `runs[0].text` is L2, as runs are.
fn key_tier(key: &str) -> Tier {
    match key {
        "id" | "group_id" | "scope" | "runs" | "mark" | "footnote" | "mark_text" | "line_break"
        | "image" | "raw" | "unknown" | "typed_value" | "lang" | "dir" => Tier::L2,
        "style" | "align" | "align_char" | "width" | "min_width" | "max_width" | "height"
        | "table_width" => Tier::L3,
        _ => Tier::L1,
    }
}

/// Drop what lies above `max`, from a value at tier `at`.
fn project(value: &mut Value, at: Tier, max: Tier) {
    match value {
        Value::Object(map) => {
            map.retain(|k, _| at.max(key_tier(k)) <= max);
            for (k, v) in map.iter_mut() {
                project(v, at.max(key_tier(k)), max);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| project(v, at, max)),
        _ => {}
    }
}

fn compare(
    path: &mut String,
    at: Tier,
    expected: Option<&Value>,
    actual: Option<&Value>,
    out: &mut Vec<Difference>,
) {
    match (expected, actual) {
        (Some(Value::Object(e)), Some(Value::Object(a))) => {
            let mut keys: Vec<&String> = e.keys().chain(a.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let len = path.len();
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(k);
                compare(path, at.max(key_tier(k)), e.get(k), a.get(k), out);
                path.truncate(len);
            }
        }
        (Some(Value::Array(e)), Some(Value::Array(a))) => {
            for i in 0..e.len().max(a.len()) {
                let len = path.len();
                path.push_str(&format!("[{i}]"));
                compare(path, at, e.get(i), a.get(i), out);
                path.truncate(len);
            }
        }
        (e, a) if e == a => {}
        (e, a) => out.push(Difference {
            tier: at,
            path: path.clone(),
            expected: e.cloned(),
            actual: a.cloned(),
        }),
    }
}

// ─── Normalizing ───

/// The full grid (every tier), from the resolved table writers draw.
fn grid(table: &ValidatedTable<'_>) -> Value {
    // As the IR has it: text transforms are styling (L3), and raw content is
    // compared as raw content, not as what a writer would make of it.
    let opts = ResolveOptions {
        transform_text: false,
        lower_html: false,
    };
    let r = resolve_with(table, opts);
    let n = Normalizer { r: &r };
    let mut grid = Map::new();
    let config = &table.config;
    if let Some(lang) = &config.lang {
        grid.insert("lang".into(), json!(lang));
    }
    if let Some(dir) = &config.dir {
        grid.insert("dir".into(), json!(dir.as_str()));
    }
    if let Some(w) = config
        .table_width
        .as_deref()
        .and_then(|w| w.parse::<Length>().ok())
    {
        grid.insert("table_width".into(), json!(w.to_string()));
    }
    let columns: Vec<Value> = r
        .columns
        .iter()
        .map(|c| {
            let mut m = Map::new();
            m.insert("id".into(), json!(c.spec.id));
            if c.is_stub {
                m.insert("stub".into(), json!(true));
            }
            m.insert("align".into(), json!(c.align.as_str()));
            if let Some(ch) = c.align_char {
                m.insert("align_char".into(), json!(ch));
            }
            for (k, v) in [
                ("width", &c.width),
                ("min_width", &c.min_width),
                ("max_width", &c.max_width),
            ] {
                if let Some(v) = v {
                    m.insert(k.into(), json!(v.to_string()));
                }
            }
            Value::Object(m)
        })
        .collect();
    grid.insert("columns".into(), Value::Array(columns));
    let h = &r.header;
    grid.insert("title".into(), n.opt_line(h.title.as_ref()));
    grid.insert("subtitle".into(), n.opt_line(h.subtitle.as_ref()));
    let extra: Vec<Value> = h.extra_lines.iter().map(|l| n.line(l)).collect();
    grid.insert("extra_lines".into(), Value::Array(extra));
    grid.insert("head".into(), n.rows(&r.head.rows, true));
    let groups: Vec<Value> = r
        .groups
        .iter()
        .map(|g| {
            let mut group = json!({
                "group_id": g.id,
                "label": n.opt_line(g.label.as_ref()),
                "rows": n.rows(&g.rows.rows, false),
                "summary_rows": n.rows(&g.summary_rows.rows, false),
            });
            // Only when the summary rows are at the top, so a grid written before
            // there was a placement still says the same.
            if g.summary_top {
                group["summary_placement"] = json!("top");
            }
            group
        })
        .collect();
    grid.insert("groups".into(), Value::Array(groups));
    let footnotes: Vec<Value> = r
        .footer
        .footnotes
        .iter()
        .map(|f| {
            let mut m = n.content(&f.content, &f.style);
            m.insert("mark".into(), json!(f.mark));
            Value::Object(m)
        })
        .collect();
    grid.insert("footnotes".into(), Value::Array(footnotes));
    let notes: Vec<Value> = r.footer.source_notes.iter().map(|l| n.line(l)).collect();
    grid.insert("source_notes".into(), Value::Array(notes));
    Value::Object(grid)
}

struct Normalizer<'r, 'a> {
    r: &'r ResolvedTable<'a>,
}

impl Normalizer<'_, '_> {
    fn opt_line(&self, line: Option<&Line<'_>>) -> Value {
        line.map_or(Value::Null, |l| self.line(l))
    }

    fn line(&self, line: &Line<'_>) -> Value {
        Value::Object(self.content(&line.content, &line.style))
    }

    fn rows(&self, rows: &[ResolvedRow<'_>], head: bool) -> Value {
        rows.iter()
            .map(|row| {
                let mut m = Map::new();
                if let Some(role) = &row.role {
                    m.insert("role".into(), json!(role.as_str()));
                }
                let cells: Vec<Value> = row.cells().map(|c| self.cell(c, head)).collect();
                m.insert("cells".into(), Value::Array(cells));
                Value::Object(m)
            })
            .collect()
    }

    fn cell(&self, c: &ResolvedCell<'_>, head: bool) -> Value {
        let mut m = Map::new();
        m.insert("col".into(), json!(c.col));
        if c.colspan > 1 {
            m.insert("colspan".into(), json!(c.colspan));
        }
        if c.rowspan > 1 {
            m.insert("rowspan".into(), json!(c.rowspan));
        }
        // The head's stub cell is the stubhead: the column says it's the stub's.
        if c.is_stub && !head {
            m.insert("stub".into(), json!(true));
        }
        if let Some(scope) = &c.scope {
            m.insert("scope".into(), json!(scope.as_str()));
        }
        m.extend(self.content(&c.content, &c.style));
        m.insert("align".into(), json!(c.align.as_str()));
        if let Some(tv) = c.typed_value {
            m.insert(
                "typed_value".into(),
                json!({"type": tv.value_type.as_str(), "value": canonical(&tv.value)}),
            );
        }
        Value::Object(m)
    }

    /// `text` (L1), `runs` (L2) and `style` (L3) of a piece of content.
    fn content(&self, nodes: &[ContentNode], style: &ResolvedStyle) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("text".into(), json!(text(nodes)));
        m.insert("runs".into(), Value::Array(self.runs(nodes)));
        if let Some(s) = style_value(style) {
            m.insert("style".into(), s);
        }
        m
    }

    fn runs(&self, nodes: &[ContentNode]) -> Vec<Value> {
        let mut runs: Vec<Value> = Vec::new();
        for node in nodes {
            let run = match node {
                ContentNode::Text { value, href } => text_run(value, None, href),
                ContentNode::StyledText {
                    value,
                    style_id,
                    href,
                } => {
                    let style = style_id.as_deref().map(|id| self.r.style(id));
                    text_run(value, style.as_ref().and_then(style_value), href)
                }
                ContentNode::LineBreak {} => json!({"line_break": true}),
                ContentNode::FootnoteMark {
                    reference,
                    mark_text,
                } => {
                    // By number, not id: the ids are the emitter's to choose.
                    let number = self
                        .r
                        .footer
                        .footnotes
                        .iter()
                        .position(|f| f.id == reference)
                        .map(|i| i + 1);
                    json!({"footnote": number, "mark_text": mark_text})
                }
                ContentNode::Image {
                    src,
                    alt,
                    width,
                    height,
                } => {
                    let mut img = Map::new();
                    img.insert("src".into(), json!(src));
                    for (k, v) in [("alt", alt), ("width", width), ("height", height)] {
                        if let Some(v) = v {
                            img.insert(k.into(), json!(v));
                        }
                    }
                    json!({ "image": img })
                }
                ContentNode::Raw { format, value } => json!({"raw": format, "text": value}),
                ContentNode::Unknown(_) => json!({"unknown": true}),
            };
            if run.get("text").and_then(Value::as_str) == Some("") && run.get("raw").is_none() {
                continue;
            }
            // Text in one style (and link) is one run, however it's split into
            // nodes.
            if let (Some(last), Some(text)) = (runs.last_mut(), run.get("text")) {
                if is_text_run(last)
                    && is_text_run(&run)
                    && last.get("style") == run.get("style")
                    && last.get("href") == run.get("href")
                {
                    let joined = format!(
                        "{}{}",
                        last["text"].as_str().unwrap_or(""),
                        text.as_str().unwrap_or("")
                    );
                    last["text"] = json!(joined);
                    continue;
                }
            }
            runs.push(run);
        }
        runs
    }
}

/// A text run; its `href` as the URL writers link to, when it is a link.
fn text_run(value: &str, style: Option<Value>, href: &Option<String>) -> Value {
    let mut m = Map::new();
    m.insert("text".into(), json!(value));
    if let Some(s) = style {
        m.insert("style".into(), s);
    }
    if let Some(url) = href.as_deref().and_then(gridwell_core::link::link_url) {
        m.insert("href".into(), json!(url));
    }
    Value::Object(m)
}

fn is_text_run(run: &Value) -> bool {
    run.as_object().is_some_and(|m| {
        m.contains_key("text")
            && m.keys()
                .all(|k| matches!(k.as_str(), "text" | "style" | "href"))
    })
}

/// The text a reader sees (L1): text nodes and line breaks, without footnote
/// marks, images or raw content. Runs of white space are one space, each line
/// is trimmed, and so is the whole.
fn text(nodes: &[ContentNode]) -> String {
    let mut s = String::new();
    for node in nodes {
        match node {
            ContentNode::Text { value, .. } | ContentNode::StyledText { value, .. } => {
                s.push_str(value)
            }
            ContentNode::LineBreak {} => s.push('\n'),
            _ => {}
        }
    }
    let lines: Vec<String> = s
        .split('\n')
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    lines.join("\n").trim().to_string()
}

/// A JSON value with integral floats written as integers: `5550.0` is `5550`,
/// however an emitter's JSON library writes it.
fn canonical(v: &Value) -> Value {
    match v {
        Value::Number(n) if !n.is_i64() && !n.is_u64() => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 9.0e15 => json!(f as i64),
            _ => v.clone(),
        },
        Value::Array(a) => Value::Array(a.iter().map(canonical).collect()),
        Value::Object(m) => {
            Value::Object(m.iter().map(|(k, v)| (k.clone(), canonical(v))).collect())
        }
        _ => v.clone(),
    }
}

/// A resolved style as its properties, `None` when it sets none. Values are
/// canonical: colors `#RRGGBB[AA]`, lengths as parsed, keywords lowercase, `700`
/// as `bold` and `400` as `normal`.
fn style_value(s: &ResolvedStyle) -> Option<Value> {
    let mut m = Map::new();
    let mut put = |k: &str, v: Option<String>| {
        if let Some(v) = v {
            m.insert(k.into(), json!(v));
        }
    };
    put("font_family", s.font_family.clone());
    put("font_size", s.font_size.as_ref().map(ToString::to_string));
    put(
        "font_weight",
        s.font_weight.as_ref().map(|w| match w {
            FontWeight::Numeric(700) => "bold".to_string(),
            FontWeight::Numeric(400) => "normal".to_string(),
            w => String::from(w.clone()),
        }),
    );
    put(
        "font_style",
        s.font_style.as_ref().map(|v| v.as_str().to_string()),
    );
    put("color", s.color.map(|c| c.to_hex()));
    put("background_color", s.background_color.map(|c| c.to_hex()));
    put(
        "text_align",
        s.text_align.as_ref().map(|v| v.as_str().to_string()),
    );
    put(
        "vertical_align",
        s.vertical_align.as_ref().map(|v| v.as_str().to_string()),
    );
    put(
        "text_transform",
        s.text_transform.as_ref().map(|v| v.as_str().to_string()),
    );
    put(
        "text_decoration",
        s.text_decoration.as_ref().map(|v| v.as_str().to_string()),
    );
    put(
        "white_space",
        s.white_space.as_ref().map(|v| v.as_str().to_string()),
    );
    put("indent", s.indent.as_ref().map(ToString::to_string));
    put(
        "word_break",
        s.word_break.as_ref().map(|v| v.as_str().to_string()),
    );
    put(
        "overflow",
        s.overflow.as_ref().map(|v| v.as_str().to_string()),
    );
    put(
        "text_overflow",
        s.text_overflow.as_ref().map(|v| v.as_str().to_string()),
    );
    put("min_width", s.min_width.as_ref().map(ToString::to_string));
    put("max_width", s.max_width.as_ref().map(ToString::to_string));
    let padding: Map<String, Value> = s
        .padding
        .iter()
        .filter_map(|(side, v)| v.as_ref().map(|v| (side.to_string(), json!(v.to_string()))))
        .collect();
    if !padding.is_empty() {
        m.insert("padding".into(), Value::Object(padding));
    }
    let mut border = Map::new();
    for ((side, edge), (_, cleared)) in s.border.iter().zip(s.border_cleared.iter()) {
        let v = match edge {
            Some(b) => {
                let mut e = Map::new();
                e.insert("style".into(), json!(b.style.as_str()));
                if let Some(w) = &b.width {
                    e.insert("width".into(), json!(w.to_string()));
                }
                if let Some(c) = b.color {
                    e.insert("color".into(), json!(c.to_hex()));
                }
                Value::Object(e)
            }
            // Removed, as opposed to left unset.
            None if *cleared => json!({"style": "none"}),
            None => continue,
        };
        border.insert(side.to_string(), v);
    }
    if !border.is_empty() {
        m.insert("border".into(), Value::Object(border));
    }
    (!m.is_empty()).then_some(Value::Object(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_cell(text: &str) -> Value {
        json!({"content": [{"type": "text", "value": text}]})
    }

    /// Two columns, a head row, a group of two rows, a title.
    fn base() -> Value {
        json!({
            "ir_version": "1.0",
            "config": {"table_cols": 2, "header_rows": 1, "body_rows": 2},
            "styles": {"defs": {}, "compositions": {}, "conditionals": []},
            "header": {"title": {"content": [{"type": "text", "value": "Title"}]}},
            "column_spec": [{"id": "a"}, {"id": "b"}],
            "table": {
                "thead": {"rows": [{"role": "column_label", "cells": [text_cell("A"), text_cell("B")]}]},
                "tbody": [{
                    "group_id": "g",
                    "label": null,
                    "rows": [
                        {"cells": [text_cell("1"), text_cell("x")]},
                        {"cells": [text_cell("2"), text_cell("y")]}
                    ],
                    "summary_rows": []
                }]
            },
            "footer": {"footnotes": [], "source_notes": []}
        })
    }

    fn table(ir: &Value) -> Table {
        Table::from_json(&ir.to_string()).unwrap()
    }

    fn grid_of(ir: &Value, tier: Tier) -> Value {
        normalize(&table(ir).validated().unwrap(), tier)
    }

    /// Check `ir` against the L3 grid of `expected`.
    fn check_against(ir: &Value, expected: &Value, tier: Option<Tier>) -> Report {
        check(&table(ir), &grid_of(expected, Tier::L3), tier).unwrap()
    }

    fn body_cell(ir: &mut Value, row: usize, col: usize) -> &mut Value {
        &mut ir["table"]["tbody"][0]["rows"][row]["cells"][col]
    }

    fn statuses(r: &Report) -> Vec<&'static str> {
        Tier::ALL
            .into_iter()
            .map(|t| r.status(t).as_str())
            .collect()
    }

    #[test]
    fn tiers_parse_either_case() {
        assert_eq!("l2".parse::<Tier>(), Ok(Tier::L2));
        assert_eq!("L0".parse::<Tier>(), Ok(Tier::L0));
        for bad in ["L4", "2", "", " L1"] {
            assert_eq!(
                bad.parse::<Tier>(),
                Err(ConformanceError::UnknownTier(bad.into())),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn styles_compare_by_their_properties_not_their_ids() {
        // One emitter: a definition, a named color, `bold`, footnote "fn1".
        let mut a = base();
        a["styles"]["defs"] =
            json!({"s1": {"color": "red", "font_weight": "bold", "font_size": "12pt"}});
        a["footer"]["footnotes"] = json!([
            {"id": "fn1", "mark": "1", "content": [{"type": "text", "value": "Note."}]}
        ]);
        let cell = body_cell(&mut a, 0, 0);
        cell["style_id"] = json!("s1");
        cell["content"] = json!([
            {"type": "text", "value": "1"},
            {"type": "footnote_mark", "ref": "fn1", "mark_text": "1"}
        ]);
        // Another: a composition over a definition, hex, `700`, other ids, and
        // its keys in another order.
        let mut b = base();
        b["styles"]["defs"] = json!({"base": {"font_size": "12pt", "color": "#ff0000"}});
        b["styles"]["compositions"] =
            json!({"zz": {"overrides": {"font_weight": 700}, "extends": "base"}});
        b["footer"]["footnotes"] = json!([
            {"content": [{"type": "text", "value": "Note."}], "mark": "1", "id": "note-a"}
        ]);
        let cell = body_cell(&mut b, 0, 0);
        cell["style_id"] = json!("zz");
        cell["content"] = json!([
            {"value": "1", "type": "text"},
            {"mark_text": "1", "ref": "note-a", "type": "footnote_mark"}
        ]);
        let ga = grid_of(&a, Tier::L3);
        assert_eq!(ga, grid_of(&b, Tier::L3));
        let cell = &ga["grid"]["groups"][0]["rows"][0]["cells"][0];
        assert_eq!(
            cell["style"],
            json!({"color": "#FF0000", "font_size": "12pt", "font_weight": "bold"})
        );
        // The mark is footnote 1, not "fn1".
        assert_eq!(
            cell["runs"],
            json!([{"text": "1"}, {"footnote": 1, "mark_text": "1"}])
        );
        assert!(check_against(&b, &a, None).passed());
    }

    #[test]
    fn a_grid_has_what_a_reader_sees() {
        let ir = base();
        let g = grid_of(&ir, Tier::L3);
        assert_eq!(g["format"], FORMAT);
        assert_eq!(g["tier"], "L3");
        let grid = &g["grid"];
        assert_eq!(grid["title"]["text"], "Title");
        assert_eq!(grid["subtitle"], Value::Null);
        assert_eq!(grid["columns"][1]["id"], "b");
        assert_eq!(grid["head"][0]["role"], "column_label");
        assert_eq!(grid["head"][0]["cells"][1]["text"], "B");
        assert_eq!(grid["groups"][0]["group_id"], "g");
        // A data row has no role; a body cell says it isn't the stub by leaving
        // `stub` out.
        let row = &grid["groups"][0]["rows"][1];
        assert_eq!(row.get("role"), None);
        assert_eq!(row["cells"][0]["col"], 0);
        assert_eq!(row["cells"][0]["text"], "2");
        assert_eq!(row["cells"][0].get("stub"), None);
        assert_eq!(row["cells"][0].get("style"), None);
    }

    #[test]
    fn text_is_collapsed_and_runs_are_merged() {
        let mut ir = base();
        ir["styles"]["defs"] =
            json!({"em": {"font_style": "italic"}, "em2": {"font_style": "italic"}});
        body_cell(&mut ir, 0, 0)["content"] = json!([
            {"type": "text", "value": "  Hel"},
            {"type": "text", "value": "lo \t there "},
            {"type": "text", "value": ""},
            {"type": "styled_text", "value": "in ", "style_id": "em"},
            {"type": "styled_text", "value": "italic", "style_id": "em2"},
            {"type": "line_break"},
            {"type": "text", "value": "  next   line "},
            {"type": "line_break"}
        ]);
        let g = grid_of(&ir, Tier::L3);
        let cell = &g["grid"]["groups"][0]["rows"][0]["cells"][0];
        assert_eq!(cell["text"], "Hello there in italic\nnext line");
        // Same text in the same style is one run, whatever the style's id; empty
        // text is no run; run text is as written.
        assert_eq!(
            cell["runs"],
            json!([
                {"text": "  Hello \t there "},
                {"text": "in italic", "style": {"font_style": "italic"}},
                {"line_break": true},
                {"text": "  next   line "},
                {"line_break": true}
            ])
        );
        // An emitter that writes empty text nodes writes the same runs as one that
        // doesn't.
        body_cell(&mut ir, 0, 1)["content"] = json!([
            {"type": "styled_text", "value": "", "style_id": "em"},
            {"type": "line_break"},
            {"type": "text", "value": ""}
        ]);
        let g = grid_of(&ir, Tier::L3);
        let cell = &g["grid"]["groups"][0]["rows"][0]["cells"][1];
        assert_eq!(cell["runs"], json!([{"line_break": true}]));
        assert_eq!(cell["text"], "");
    }

    #[test]
    fn marks_images_and_raw_are_runs_but_not_text() {
        let mut ir = base();
        ir["footer"]["footnotes"] = json!([
            {"id": "z", "mark": "a", "content": [{"type": "text", "value": "First."}]},
            {"id": "y", "mark": "b", "content": [{"type": "text", "value": "Second."}]}
        ]);
        body_cell(&mut ir, 0, 1)["content"] = json!([
            {"type": "text", "value": "x"},
            {"type": "footnote_mark", "ref": "y", "mark_text": "b"},
            {"type": "image", "src": "data:image/png;base64,AAAA", "alt": "dot", "width": "4px"},
            {"type": "raw", "format": "html", "value": "<b>raw</b>"}
        ]);
        let g = grid_of(&ir, Tier::L3);
        let cell = &g["grid"]["groups"][0]["rows"][0]["cells"][1];
        assert_eq!(cell["text"], "x");
        assert_eq!(
            cell["runs"],
            json!([
                {"text": "x"},
                {"footnote": 2, "mark_text": "b"},
                {"image": {"src": "data:image/png;base64,AAAA", "alt": "dot", "width": "4px"}},
                {"raw": "html", "text": "<b>raw</b>"}
            ])
        );
        assert_eq!(g["grid"]["footnotes"][1]["mark"], "b");
        assert_eq!(g["grid"]["footnotes"][1]["text"], "Second.");
        // L2 has the image without its size (L3).
        let g2 = grid_of(&ir, Tier::L2);
        assert_eq!(
            g2["grid"]["groups"][0]["rows"][0]["cells"][1]["runs"][2],
            json!({"image": {"src": "data:image/png;base64,AAAA", "alt": "dot"}})
        );
    }

    #[test]
    fn spans_give_one_cell_and_covered_slots_none() {
        let mut ir = base();
        ir["table"]["thead"]["rows"][0]["cells"] = json!([
            {"content": [{"type": "text", "value": "Both"}], "colspan": 2},
            {"content": [], "is_placeholder": true}
        ]);
        let g = grid_of(&ir, Tier::L1);
        assert_eq!(
            g["grid"]["head"][0]["cells"],
            json!([{"col": 0, "colspan": 2, "text": "Both"}])
        );
    }

    #[test]
    fn the_stub_groups_and_summary_rows() {
        let mut ir = base();
        ir["config"]["stub_cols"] = json!(1);
        let group = &mut ir["table"]["tbody"][0];
        group["label"] = json!({"content": [{"type": "text", "value": "Group"}]});
        group["summary_rows"] =
            json!([{"role": "summary", "cells": [text_cell("Total"), text_cell("3")]}]);
        let g = grid_of(&ir, Tier::L1);
        let grid = &g["grid"];
        assert_eq!(grid["columns"], json!([{"stub": true}, {}]));
        assert_eq!(grid["groups"][0]["label"], json!({"text": "Group"}));
        assert_eq!(
            grid["groups"][0]["rows"][0]["cells"][0],
            json!({"col": 0, "stub": true, "text": "1"})
        );
        // The role as the IR names it, whichever spelling was written.
        assert_eq!(
            grid["groups"][0]["summary_rows"],
            json!([{"role": "summary_row", "cells": [
                {"col": 0, "stub": true, "text": "Total"},
                {"col": 1, "text": "3"}
            ]}])
        );
        // The stubhead isn't a stub cell: the column says it's the stub's.
        assert_eq!(grid["head"][0]["cells"][0], json!({"col": 0, "text": "A"}));
        // Where the summary rows are: said only when at the top, and checked at L1.
        assert!(grid["groups"][0].get("summary_placement").is_none());
        let mut top = ir.clone();
        top["table"]["tbody"][0]["summary_placement"] = json!("top");
        let t = grid_of(&top, Tier::L1);
        assert_eq!(t["grid"]["groups"][0]["summary_placement"], "top");
        let r = check(&table(&top), &g, None).unwrap();
        assert_eq!(statuses(&r), ["pass", "fail", "not checked", "not checked"]);
        assert_eq!(r.differences[0].path, "groups[0].summary_placement");
        top["table"]["tbody"][0]["summary_placement"] = json!("bottom");
        assert!(check(&table(&top), &g, None).unwrap().passed());
    }

    #[test]
    fn style_values_are_canonical() {
        let mut ir = base();
        ir["styles"]["defs"] = json!({"s": {
            "background_color": "#abc",
            "font_weight": 400,
            "padding": {"top": "4px", "left": "0.5em"},
            "border": {"top": {"style": "solid", "width": "1px", "color": "black"},
                       "bottom": {"style": "none"}},
            "text_align": "CENTER"
        }});
        body_cell(&mut ir, 1, 1)["style_id"] = json!("s");
        let g = grid_of(&ir, Tier::L3);
        let cell = &g["grid"]["groups"][0]["rows"][1]["cells"][1];
        assert_eq!(
            cell["style"],
            json!({
                "background_color": "#AABBCC",
                "font_weight": "normal",
                "padding": {"top": "4px", "left": "0.5em"},
                "border": {"top": {"style": "solid", "width": "1px", "color": "#000000"},
                           "bottom": {"style": "none"}},
                "text_align": "center"
            })
        );
    }

    #[test]
    fn integral_numbers_are_integers() {
        let mut a = base();
        body_cell(&mut a, 0, 0)["typed_value"] = json!({"type": "number", "value": 5550});
        let mut b = base();
        body_cell(&mut b, 0, 0)["typed_value"] = json!({"type": "number", "value": 5550.0});
        assert_eq!(grid_of(&a, Tier::L2), grid_of(&b, Tier::L2));
        let mut c = base();
        body_cell(&mut c, 0, 0)["typed_value"] = json!({"type": "number", "value": 5550.5});
        assert!(!check_against(&c, &a, None).passed());
    }

    #[test]
    fn each_tier_writes_its_own_and_those_below() {
        let mut ir = base();
        ir["styles"]["defs"] = json!({"s": {"color": "red"}});
        body_cell(&mut ir, 0, 0)["style_id"] = json!("s");
        body_cell(&mut ir, 0, 0)["typed_value"] = json!({"type": "number", "value": 1});
        let keys = |tier| {
            let mut found = std::collections::BTreeSet::new();
            fn walk(v: &Value, found: &mut std::collections::BTreeSet<String>) {
                match v {
                    Value::Object(m) => m.iter().for_each(|(k, v)| {
                        found.insert(k.clone());
                        walk(v, found)
                    }),
                    Value::Array(a) => a.iter().for_each(|v| walk(v, found)),
                    _ => {}
                }
            }
            walk(&grid_of(&ir, tier)["grid"], &mut found);
            found
        };
        let (l1, l2, l3) = (keys(Tier::L1), keys(Tier::L2), keys(Tier::L3));
        for k in ["text", "col", "cells", "columns", "groups"] {
            assert!(l1.contains(k), "{k}");
        }
        for k in ["runs", "id", "group_id", "typed_value"] {
            assert!(!l1.contains(k) && l2.contains(k), "{k}");
        }
        for k in ["style", "align"] {
            assert!(!l2.contains(k) && l3.contains(k), "{k}");
        }
        assert_eq!(grid_of(&ir, Tier::L0)["grid"], json!({}));
    }

    #[test]
    fn a_difference_fails_its_tier_and_those_above() {
        let expected = base();
        // A style: L3.
        let mut styled = base();
        styled["styles"]["defs"] = json!({"s": {"color": "red"}});
        body_cell(&mut styled, 0, 0)["style_id"] = json!("s");
        let r = check_against(&styled, &expected, None);
        assert_eq!(statuses(&r), ["pass", "pass", "pass", "fail"]);
        assert_eq!(r.differences.len(), 1);
        let d = &r.differences[0];
        assert_eq!(d.tier, Tier::L3);
        assert_eq!(d.path, "groups[0].rows[0].cells[0].style");
        assert_eq!(d.expected, None);
        assert_eq!(d.actual, Some(json!({"color": "#FF0000"})));
        // Not checked above L2, it passes.
        let r = check_against(&styled, &expected, Some(Tier::L2));
        assert!(r.passed());
        assert_eq!(statuses(&r), ["pass", "pass", "pass", "not checked"]);
        assert!(r.differences.is_empty());
        // A column id: L2.
        let mut renamed = base();
        renamed["column_spec"][1]["id"] = json!("other");
        let r = check_against(&renamed, &expected, None);
        assert_eq!(statuses(&r), ["pass", "pass", "fail", "fail"]);
        assert_eq!(r.differences[0].path, "columns[1].id");
        // Text: L1 (and its run, L2).
        let mut retexted = base();
        body_cell(&mut retexted, 1, 1)["content"] = json!([{"type": "text", "value": "z"}]);
        let r = check_against(&retexted, &expected, None);
        assert_eq!(statuses(&r), ["pass", "fail", "fail", "fail"]);
        let paths: Vec<(Tier, &str)> = r
            .differences
            .iter()
            .map(|d| (d.tier, d.path.as_str()))
            .collect();
        assert_eq!(
            paths,
            [
                (Tier::L2, "groups[0].rows[1].cells[1].runs[0].text"),
                (Tier::L1, "groups[0].rows[1].cells[1].text"),
            ]
        );
        assert!(!r.passed());
    }

    #[test]
    fn a_missing_or_extra_item_is_a_difference() {
        let expected = base();
        let mut longer = base();
        longer["config"]["body_rows"] = json!(3);
        longer["table"]["tbody"][0]["rows"]
            .as_array_mut()
            .unwrap()
            .push(json!({"cells": [text_cell("3"), text_cell("z")]}));
        let r = check_against(&longer, &expected, Some(Tier::L1));
        assert_eq!(r.differences.len(), 1);
        let d = &r.differences[0];
        assert_eq!((d.tier, d.path.as_str()), (Tier::L1, "groups[0].rows[2]"));
        assert_eq!(d.expected, None);
        assert_eq!(
            d.actual,
            Some(json!({"cells": [{"col": 0, "text": "3"}, {"col": 1, "text": "z"}]}))
        );
        // The other way round.
        let r = check(&table(&expected), &grid_of(&longer, Tier::L1), None).unwrap();
        assert_eq!(r.differences[0].expected.as_ref().map(|_| ()), Some(()));
        assert_eq!(r.differences[0].actual, None);
        assert_eq!(
            r.differences[0].to_string(),
            "L1 groups[0].rows[2]: expected {\"cells\":[{\"col\":0,\"text\":\"3\"},{\"col\":1,\"text\":\"z\"}]}, got nothing"
        );
    }

    #[test]
    fn an_expected_grid_is_checked_only_to_its_tier() {
        let ir = base();
        let l1 = grid_of(&ir, Tier::L1);
        // A style the L1 grid can't see.
        let mut styled = base();
        styled["styles"]["defs"] = json!({"s": {"color": "red"}});
        body_cell(&mut styled, 0, 0)["style_id"] = json!("s");
        let r = check(&table(&styled), &l1, None).unwrap();
        assert_eq!(r.tier, Tier::L1);
        assert_eq!(statuses(&r), ["pass", "pass", "not checked", "not checked"]);
        assert_eq!(
            check(&table(&ir), &l1, Some(Tier::L2)),
            Err(ConformanceError::TierNotSpecified {
                requested: Tier::L2,
                specified: Tier::L1
            })
        );
        // Other top-level keys are for people.
        let mut annotated = l1.clone();
        annotated["case"] = json!("anything");
        annotated["source"] = json!({"L1": "somewhere"});
        assert!(check(&table(&ir), &annotated, None).unwrap().passed());
    }

    #[test]
    fn an_invalid_table_fails_l0() {
        let expected = grid_of(&base(), Tier::L3);
        let mut invalid = base();
        invalid["config"]["table_cols"] = json!(3);
        let r = check(&table(&invalid), &expected, None).unwrap();
        assert_eq!(statuses(&r), ["fail", "fail", "fail", "fail"]);
        assert!(!r.errors.is_empty());
        assert!(r.differences.is_empty());
        // IR that doesn't parse.
        let r = check_json("{not json", &expected.to_string(), Some(Tier::L1)).unwrap();
        assert_eq!(statuses(&r), ["fail", "fail", "not checked", "not checked"]);
        assert_eq!(r.errors.len(), 1);
        assert!(
            r.errors[0].starts_with("the IR doesn't parse: "),
            "{}",
            r.errors[0]
        );
        let text = r.to_string();
        assert!(
            text.starts_with("fails L0 (checked to L1)\n  L0 valid: fail (1 error)\n"),
            "{text}"
        );
        // Still, a tier the grid doesn't go to is the caller's error.
        let l1 = grid_of(&base(), Tier::L1).to_string();
        assert!(matches!(
            check_json("{not json", &l1, Some(Tier::L3)),
            Err(ConformanceError::TierNotSpecified { .. })
        ));
    }

    #[test]
    fn an_expected_grid_that_isnt_one_is_an_error() {
        let ir = base().to_string();
        let grid = grid_of(&base(), Tier::L3);
        let with = |k: &str, v: Value| {
            let mut g = grid.clone();
            g[k] = v;
            g.to_string()
        };
        let without = |k: &str| {
            let mut g = grid.clone();
            g.as_object_mut().unwrap().remove(k);
            g.to_string()
        };
        let cases = [
            ("[]".to_string(), "isn't a JSON object"),
            ("nope".to_string(), "isn't JSON: "),
            (
                with("format", json!("gridwell-conformance-grid/2")),
                "has format \"gridwell-conformance-grid/2\"",
            ),
            (without("format"), "has no format"),
            (with("tier", json!("L9")), "has an unknown tier \"L9\""),
            (without("tier"), "has no tier"),
            (with("grid", json!([])), "has no grid object"),
            (without("grid"), "has no grid object"),
        ];
        for (expected, why) in cases {
            match check_json(&ir, &expected, None) {
                Err(ConformanceError::NotAGrid(msg)) => assert!(msg.starts_with(why), "{msg}"),
                other => panic!("{expected}: {other:?}"),
            }
        }
    }

    #[test]
    fn the_report_as_text_and_json() {
        let expected = base();
        let mut other = base();
        for row in 0..2 {
            for col in 0..2 {
                body_cell(&mut other, row, col)["content"] =
                    json!([{"type": "text", "value": "q"}]);
            }
        }
        let r = check_against(&other, &expected, None);
        let text = r.to_string();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[..6],
            [
                "fails L1 (checked to L3)",
                "  L0 valid: pass",
                "  L1 structure: fail (4 differences)",
                "  L2 content: fail (4 differences)",
                "  L3 styling: fail",
                "8 differences:",
            ]
        );
        assert_eq!(
            lines[7],
            "  L1 groups[0].rows[0].cells[0].text: expected \"1\", got \"q\""
        );
        let j = r.to_json();
        assert_eq!(j["passed"], false);
        assert_eq!(j["tier"], "L3");
        assert_eq!(
            j["tiers"],
            json!({"L0": "pass", "L1": "fail", "L2": "fail", "L3": "fail"})
        );
        assert_eq!(j["errors"], json!([]));
        assert_eq!(
            j["differences"][1],
            json!({"tier": "L1", "path": "groups[0].rows[0].cells[0].text", "expected": "1", "actual": "q"})
        );
        assert_eq!(j["text"], text);
        // Absent is no key; null is null.
        let mut titled = base();
        titled["header"]["title"] = Value::Null;
        let r = check_against(&titled, &expected, Some(Tier::L1));
        assert_eq!(
            r.to_json()["differences"],
            json!([{"tier": "L1", "path": "title", "expected": {"text": "Title"}, "actual": null}])
        );
        let r = check_against(&expected, &expected, None);
        assert_eq!(
            r.to_string(),
            "passes L3\n  L0 valid: pass\n  L1 structure: pass\n  L2 content: pass\n  L3 styling: pass\n"
        );
    }

    #[test]
    fn long_reports_are_cut_short() {
        let mut expected = base();
        let mut other = base();
        let rows = 30;
        for ir in [&mut expected, &mut other] {
            ir["config"]["body_rows"] = json!(rows);
        }
        let row = |t: &str| json!({"cells": [text_cell(t), text_cell(t)]});
        expected["table"]["tbody"][0]["rows"] = (0..rows).map(|_| row("a")).collect();
        other["table"]["tbody"][0]["rows"] = (0..rows).map(|_| row("b")).collect();
        let r = check_against(&other, &expected, Some(Tier::L1));
        assert_eq!(r.differences.len(), 60);
        let text = r.to_string();
        assert_eq!(
            text.lines()
                .filter(|l| l.starts_with("  L1 groups"))
                .count(),
            SHOWN
        );
        assert!(text.ends_with("  … and 40 more\n"), "{text}");
        // And long values clipped.
        let d = Difference {
            tier: Tier::L1,
            path: "p".into(),
            expected: Some(json!("x".repeat(200))),
            actual: None,
        };
        let s = d.to_string();
        assert!(s.contains(&format!("\"{}…", "x".repeat(119))), "{s}");
    }
}
