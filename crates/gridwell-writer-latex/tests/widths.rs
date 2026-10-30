//! Column widths in the source: fixed and `%` columns are `p{…}` columns, `fr`
//! columns tabularx's `X` (worked out in a longtable), each aligned by its
//! paragraphs. The PDF measurements are in `compile.rs`.

use gridwell_ir::{StyleDef, Table};
use gridwell_testkit::{cell, column, placeholder, row, CellExt, ColumnExt, TableBuilder};
use gridwell_writer_latex::{render_latex, LatexWriter, LatexWriterConfig};

fn table(widths: &[&str], table_width: Option<&str>) -> Table {
    let mut tb = TableBuilder::new(widths.len() as u32).columns(
        widths
            .iter()
            .enumerate()
            .map(|(i, w)| column(&format!("c{i}"), "H").width(w))
            .collect(),
    );
    if let Some(w) = table_width {
        tb = tb.table_width(w);
    }
    tb.body(vec![row(widths.iter().map(|_| cell("x")).collect())])
        .build()
}

/// The `\begin{…}` line.
fn begin(t: &Table) -> String {
    begin_with(t, false)
}

fn begin_with(t: &Table, longtable: bool) -> String {
    let src = LatexWriter::with_config(LatexWriterConfig {
        longtable,
        ..Default::default()
    })
    .render(t)
    .unwrap();
    src.lines()
        .find(|l| l.starts_with("\\begin{"))
        .unwrap()
        .to_string()
}

const LEFT: &str = ">{\\raggedright\\arraybackslash}";

#[test]
fn natural_and_fixed_columns_stay_in_a_tabular() {
    assert_eq!(
        begin(&table(&["auto", "auto"], None)),
        "\\begin{tabular}{ll}"
    );
    assert_eq!(
        begin(&table(&["120px", "2cm"], Some("50%"))),
        format!("\\begin{{tabular}}{{{LEFT}p{{90pt}}{LEFT}p{{2cm}}}}")
    );
    // Widths that aren't widths are natural.
    assert_eq!(
        begin(&table(&["0fr", "-5%", "0%", "wide"], None)),
        "\\begin{tabular}{llll}"
    );
}

#[test]
fn percent_columns_are_shares_of_the_width_less_padding() {
    let share = |p: &str| format!("{LEFT}p{{{p}\\dimexpr \\linewidth-6\\tabcolsep\\relax}}");
    assert_eq!(
        begin(&table(&["25%", "75%", "auto"], None)),
        format!("\\begin{{tabular}}{{{}{}l}}", share("0.25"), share("0.75"))
    );
    // `table_width` is the width they divide.
    for (width, w) in [
        ("100%", "\\linewidth"),
        ("50%", "0.5\\linewidth"),
        ("400px", "300pt"),
        ("6in", "6in"),
        ("auto", "\\linewidth"),
    ] {
        assert_eq!(
            begin(&table(&["50%"], Some(width))),
            format!("\\begin{{tabular}}{{{LEFT}p{{0.5\\dimexpr {w}-2\\tabcolsep\\relax}}}}"),
            "{width}"
        );
    }
}

#[test]
fn fr_columns_are_tabularx_columns_weighted_by_hsize() {
    assert_eq!(
        begin(&table(&["1fr", "1fr", "auto"], None)),
        format!("\\begin{{tabularx}}{{\\linewidth}}{{{LEFT}X{LEFT}Xl}}")
    );
    // The `\hsize` factors add up to the number of `X` columns.
    assert_eq!(
        begin(&table(&["1fr", "3fr", "40pt"], Some("50%"))),
        "\\begin{tabularx}{0.5\\linewidth}{\
         >{\\raggedright\\arraybackslash\\hsize=0.5\\hsize}X\
         >{\\raggedright\\arraybackslash\\hsize=1.5\\hsize}X\
         >{\\raggedright\\arraybackslash}p{40pt}}"
    );
}

#[test]
fn longtable_fr_columns_share_what_fixed_and_percent_columns_leave() {
    let content = "\\dimexpr \\linewidth-8\\tabcolsep\\relax";
    assert_eq!(
        begin_with(&table(&["1fr", "20%", "3fr", "30pt"], None), true),
        format!(
            "\\begin{{longtable}}[l]{{\
             {LEFT}p{{0.25\\dimexpr 0.8{content}-30pt\\relax}}\
             {LEFT}p{{0.2{content}}}\
             {LEFT}p{{0.75\\dimexpr 0.8{content}-30pt\\relax}}\
             {LEFT}p{{30pt}}}}"
        )
    );
}

#[test]
fn paragraph_columns_follow_their_alignment() {
    let mut t = table(&["1fr", "1fr", "1fr", "1fr", "10pt"], None);
    for (c, a) in t
        .column_spec
        .iter_mut()
        .zip(["center", "right", "justify", "left", "right"])
    {
        c.align = a.into();
    }
    assert_eq!(
        begin(&t),
        "\\begin{tabularx}{\\linewidth}{\
         >{\\centering\\arraybackslash}X\
         >{\\raggedleft\\arraybackslash}X\
         X\
         >{\\raggedright\\arraybackslash}X\
         >{\\raggedleft\\arraybackslash}p{10pt}}"
    );
}

#[test]
fn hidden_columns_take_no_padding_or_share() {
    let mut t = table(&["50%", "1fr", "2fr"], None);
    t.column_spec[2].hidden = true;
    assert_eq!(
        begin(&t),
        format!(
            "\\begin{{tabularx}}{{\\linewidth}}{{\
             {LEFT}p{{0.5\\dimexpr \\linewidth-4\\tabcolsep\\relax}}{LEFT}X}}"
        )
    );
}

#[test]
fn a_cell_aligned_apart_keeps_its_columns_width() {
    let t = TableBuilder::new(3)
        .style_def(
            "r",
            StyleDef {
                text_align: Some("right".into()),
                ..Default::default()
            },
        )
        .style_def(
            "l",
            StyleDef {
                text_align: Some("left".into()),
                ..Default::default()
            },
        )
        .columns(vec![
            column("a", "A").width("1fr"),
            column("b", "B").width("3fr"),
            column("c", "C").width("20pt"),
        ])
        .body(vec![
            row(vec![
                cell("ra").style("r"),
                cell("rb").style("r"),
                cell("rc").style("r"),
            ]),
            // The column's own alignment needs no `\multicolumn`.
            row(vec![
                cell("la").style("l"),
                cell("lb").style("l"),
                cell("lc").style("l"),
            ]),
            row(vec![cell("span").colspan(2), placeholder(), cell("c")]),
        ])
        .build();
    let src = render_latex(&t).unwrap();
    let right = ">{\\raggedleft\\arraybackslash";
    assert!(
        src.contains(&format!(
            "\\multicolumn{{1}}{{{right}\\hsize=0.5\\hsize}}X}}{{ra}} & \
             \\multicolumn{{1}}{{{right}\\hsize=1.5\\hsize}}X}}{{rb}} & \
             \\multicolumn{{1}}{{{right}}}p{{20pt}}}}{{rc}} \\\\"
        )),
        "{src}"
    );
    assert!(src.contains("\nla & lb & lc \\\\"), "{src}");
    assert!(src.contains("\\multicolumn{2}{l}{span} & c \\\\"), "{src}");
}
