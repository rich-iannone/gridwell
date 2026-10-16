//! The base stylesheet: gridwell's default HTML look, modeled on gt's (fonts, the
//! rules above and below the table and around the column labels, row-group labels,
//! the stub, footnotes and source notes, padding). It is the HTML counterpart of
//! LaTeX's default booktabs rules: chosen by table structure, never by data.
//!
//! Every selector is wrapped in `:where()`, which has zero specificity, so the
//! table's own styles (class rules) and any host-page CSS always win over it.
//! Rules target the part classes (`{prefix}_title`, `{prefix}_stub`, …).
//!
//! Cells never set an inherited property (font weight, alignment, color): a value
//! set directly on a cell beats one it would inherit from a row style on its
//! `<tr>`, whatever the specificity. Such properties go on the container or the
//! row instead, and `th` inherits its weight rather than being bold.

/// Text color for everything (gt's default).
const TEXT: &str = "#333333";
/// The darker rules above a heading and below a footer.
const OUTER_RULE: &str = "2px solid #A8A8A8";
/// The rules at the table's own top and bottom (where there's no heading or
/// footer, these are the outermost rules, as in gt), around the column labels and
/// group labels.
const SECTION_RULE: &str = "2px solid #D3D3D3";
/// The hairline between body rows.
const ROW_RULE: &str = "1px solid #D3D3D3";

/// The base rules for class prefix `p`, one per line, in order.
pub fn rules(p: &str) -> Vec<String> {
    let w = |selector: &str, decls: &str| format!(":where({selector}) {{ {decls} }}");
    vec![
        // The container shrinks to the table and is centered, so the heading and
        // footer are as wide as the table (gt puts both inside its table).
        w(
            &format!(".{p}_container"),
            &format!(
                "font-family: system-ui, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; \
                 color: {TEXT}; font-size: 16px; line-height: normal; \
                 -webkit-font-smoothing: antialiased; -moz-osx-font-smoothing: grayscale; \
                 width: fit-content; max-width: 100%; margin-left: auto; margin-right: auto"
            ),
        ),
        // The first heading line (the heading is the container's first divs)
        // carries the top rule.
        w(
            &format!(
                ".{p}_title:first-of-type, .{p}_subtitle:first-of-type, \
                 .{p}_header_line:first-of-type"
            ),
            &format!("border-top: {OUTER_RULE}"),
        ),
        w(
            &format!(".{p}_title"),
            "font-size: 125%; text-align: center; padding: 4px 5px",
        ),
        w(
            &format!(".{p}_subtitle"),
            "font-size: 85%; text-align: center; padding: 3px 5px 5px",
        ),
        w(
            &format!(".{p}_header_line"),
            "font-size: 85%; text-align: center; padding: 0 5px 5px",
        ),
        w(
            &format!(".{p}_table"),
            &format!(
                "border-collapse: collapse; margin: 0; border-top: {SECTION_RULE}; \
                 border-bottom: {SECTION_RULE}"
            ),
        ),
        w(
            &format!(".{p}_table td, .{p}_table th"),
            &format!("padding: 8px 5px; border-top: {ROW_RULE}"),
        ),
        // Column labels are normal weight (not the browser's bold), unless a row
        // style says otherwise.
        w(&format!(".{p}_table th"), "font-weight: inherit"),
        // Column labels and spanners: no hairlines between head rows, bottom
        // aligned (on the row, so a row style can change it), a rule under the head.
        w(
            &format!(".{p}_table thead th"),
            "border-top: none; padding: 5px 5px 6px",
        ),
        w(&format!(".{p}_table thead tr"), "vertical-align: bottom"),
        w(
            &format!(".{p}_table thead"),
            &format!("border-bottom: {SECTION_RULE}"),
        ),
        // A spanner's rule is inset 4px at each end, so neighboring spanners' rules
        // don't run together (a background line: a border can't be inset).
        w(
            &format!(".{p}_spanner"),
            "padding: 5px 4px 7px; background-image: linear-gradient(#D3D3D3, #D3D3D3); \
             background-size: calc(100% - 8px) 2px; background-position: bottom center; \
             background-repeat: no-repeat",
        ),
        w(
            &format!(".{p}_group_label"),
            &format!("border-top: {SECTION_RULE}; border-bottom: {SECTION_RULE}"),
        ),
        w(
            &format!(".{p}_stub"),
            &format!("border-right: {SECTION_RULE}"),
        ),
        // The first grand summary row gets gt's double rule.
        w(
            &format!(
                "tbody > tr.{p}_grand_summary:first-child td, \
                 tr:not(.{p}_grand_summary) + tr.{p}_grand_summary td"
            ),
            "border-top: 6px double #D3D3D3",
        ),
        // Numbers line up.
        w(
            &format!(".{p}_table td.{p}__al_right"),
            "font-variant-numeric: tabular-nums",
        ),
        w(
            &format!(".{p}_footer"),
            &format!("border-bottom: {OUTER_RULE}"),
        ),
        w(
            &format!(".{p}_footnote, .{p}_source_note"),
            "margin: 0; font-size: 90%; padding: 4px 5px",
        ),
        // Marks: small and raised (not a full superscript), italic, and not drawn
        // as links.
        w(
            &format!(".{p}_footnote_mark"),
            "font-size: 75%; vertical-align: 0.4em; line-height: 0; font-style: italic",
        ),
        w(
            &format!(".{p}_footnote_mark a"),
            "color: inherit; text-decoration: none",
        ),
    ]
}
