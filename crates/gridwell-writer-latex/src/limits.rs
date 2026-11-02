//! What LaTeX output can't express (see `gridwell_layout::unsupported`).

use gridwell_ir::content::ContentNode;
use gridwell_ir::{BorderStyle, Table};
use gridwell_layout::unsupported::{find, find_table, Check, TableCheck, Unsupported};
use gridwell_layout::{resolve, CellVAlign, ResolvedCell, ResolvedTable};

use crate::render::{monospace, BASE_PT};

const TABLE_CHECKS: &[TableCheck] = &[
    TableCheck {
        feature: "lang",
        reason: "a language is the document's, set up by babel or polyglossia in the \
                 preamble the table goes into",
        uses: |rt| rt.table.config.lang.is_some(),
    },
    TableCheck {
        feature: "rtl",
        reason: "right to left needs the bidirectional support of babel or polyglossia, set \
                 up in the document's preamble; the table is written left to right",
        uses: |rt| rt.is_rtl(),
    },
];

const CHECKS: &[Check] = &[
    Check {
        feature: "image",
        reason: "a LaTeX fragment can't carry an image's data (`\\includegraphics` reads a \
                 file): an image shows its alt text",
        uses: |_, p| p.images().next().is_some(),
    },
    Check {
        feature: "font-family",
        reason: "a named system font needs XeLaTeX/LuaLaTeX with fontspec; the output also \
                 targets pdfLaTeX (monospace families map to `\\texttt`)",
        // A cell or group label in a monospace family is `\texttt`; nothing
        // else takes a family at all.
        uses: |rt, p| {
            let named = !p.style.font_names().is_empty();
            named && !(p.in_grid() && monospace(p.style))
                || p.runs(rt).any(|s| !s.font_names().is_empty())
        },
    },
    Check {
        feature: "vertical-align",
        reason: "a tabular row sets its cells on one baseline; `m`/`b` columns align whole \
                 columns, not single cells",
        // A row's cells sit at its top, as if top-aligned; so a cell aligned
        // otherwise only shows it beside a cell that may be taller. A cell
        // spanning rows is a `\multirow`, centered on them: only bottom shows.
        // A group label is alone in its row.
        uses: |rt, p| {
            let Some((cell, row)) = p.cell else {
                return false;
            };
            let valign = cell.style.cell_valign();
            if cell.rowspan > 1 {
                return valign == Some(CellVAlign::Bottom);
            }
            let size = |c: &ResolvedCell| c.style.size_pt(BASE_PT).unwrap_or(BASE_PT);
            let taller = |c: &ResolvedCell| tall(rt, c) || size(c) > size(cell);
            matches!(valign, Some(CellVAlign::Middle | CellVAlign::Bottom))
                && row.cells().any(|c| !std::ptr::eq(c, cell) && taller(c))
        },
    },
    Check {
        feature: "border-styles",
        reason: "a vertical rule is always solid: dashed and dotted left and right borders \
                 are drawn solid",
        uses: |_, p| {
            let s = p.style.strokes(BASE_PT);
            p.in_grid()
                && [s.left, s.right]
                    .iter()
                    .flatten()
                    .any(|k| matches!(k.style, BorderStyle::Dashed | BorderStyle::Dotted))
        },
    },
];

/// Whether a cell may make its row more than a line high: it breaks lines, has
/// raw LaTeX, sits in a column with a width (which wraps), or has a run in a
/// size of its own. (A cell spanning rows overflows into the rows below.)
fn tall(rt: &ResolvedTable, c: &ResolvedCell) -> bool {
    let columns = rt.columns.get(c.col..c.col + c.colspan).unwrap_or(&[]);
    columns.iter().any(|col| col.width.is_some())
        || c.content.iter().any(|n| match n {
            ContentNode::LineBreak {} => true,
            ContentNode::Raw { format, .. } => format == "latex",
            ContentNode::StyledText {
                style_id: Some(id), ..
            } => rt.style(id).font_size.is_some(),
            _ => false,
        })
}

/// The features of `table` that its LaTeX output can't express, and where.
pub fn unsupported(table: &Table) -> Vec<Unsupported> {
    let rt = resolve(table);
    let mut found = find_table(&rt, TABLE_CHECKS);
    found.extend(find(&rt, CHECKS));
    found
}
