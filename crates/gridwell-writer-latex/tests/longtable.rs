//! A longtable's heads and feet: the title lines are rows of its first head (so
//! a page break can't strand them), the column labels head every page, a rule
//! ends every page, and the notes are rows of the last foot. The PDF over pages
//! is in `compile.rs`.

use gridwell_ir::Table;
use gridwell_testkit::{cell, column, row, TableBuilder};
use gridwell_writer_latex::{LatexWriter, LatexWriterConfig};

fn table(title: bool, notes: bool) -> Table {
    let mut tb = TableBuilder::new(2)
        .columns(vec![column("a", "A"), column("b", "B")])
        .head(row(vec![cell("A"), cell("B")]));
    if title {
        tb = tb.title("T").subtitle("S");
    }
    if notes {
        tb = tb.footnote("f1", "a", "F").source_note("N");
    }
    tb.body(vec![row(vec![cell("x"), cell("y")])]).build()
}

fn render(t: &Table, booktabs: bool) -> String {
    LatexWriter::with_config(LatexWriterConfig {
        longtable: true,
        booktabs,
        ..Default::default()
    })
    .render(t)
    .unwrap()
}

fn wide(text: &str, pos: char) -> String {
    format!(
        "\\multicolumn{{2}}{{@{{}}l@{{}}}}{{\\makebox[0pt][l]{{\\parbox[{pos}]{{\\linewidth}}\
         {{\\raggedright {text}}}}}}} \\\\"
    )
}

#[test]
fn title_and_notes_are_rows_of_the_first_head_and_last_foot() {
    let src = render(&table(true, true), true);
    let head = "\\toprule\nA & B \\\\\n\\midrule\n";
    let want = [
        "\\begin{longtable}[l]{ll}\n".to_string(),
        format!("{}\n", wide("{\\large\\bfseries T}", 'b')),
        format!("{}[6pt]\n", wide("{\\small S}", 'b')),
        format!("{head}\\endfirsthead\n"),
        format!("{head}\\endhead\n"),
        "\\bottomrule\n\\endfoot\n".to_string(),
        "\\bottomrule\n".to_string(),
        format!("{}\n", wide("\\textsuperscript{a} F", 't')),
        format!("{}\n", wide("{\\footnotesize N}", 't')),
        "\\endlastfoot\n".to_string(),
        "x & y \\\\\n\\end{longtable}\n".to_string(),
    ]
    .concat();
    assert_eq!(src, want);
}

#[test]
fn without_a_title_the_head_is_one_head_and_without_notes_one_foot() {
    let src = render(&table(false, false), true);
    assert_eq!(
        src,
        "\\begin{longtable}[l]{ll}\n\\toprule\nA & B \\\\\n\\midrule\n\\endhead\n\
         \\bottomrule\n\\endfoot\nx & y \\\\\n\\end{longtable}\n"
    );
    // Each is independent of the other.
    let titled = render(&table(true, false), true);
    assert!(titled.contains("\\endfirsthead") && !titled.contains("\\endlastfoot"));
    let noted = render(&table(false, true), true);
    assert!(!noted.contains("\\endfirsthead") && noted.contains("\\endlastfoot"));
}

#[test]
fn nothing_is_outside_the_longtable() {
    for (title, notes) in [(true, true), (true, false), (false, true)] {
        let src = render(&table(title, notes), true);
        assert!(src.starts_with("\\begin{longtable}"), "{src}");
        assert!(src.ends_with("\\end{longtable}\n"), "{src}");
    }
}

#[test]
fn without_booktabs_pages_end_with_hline() {
    let src = render(&table(true, true), false);
    assert!(
        src.contains("\\endhead\n\\hline\n\\endfoot\n\\hline\n\\multicolumn{2}"),
        "{src}"
    );
    assert!(!src.contains("rule"), "{src}");
}

#[test]
fn without_column_labels_each_page_starts_with_the_top_rule() {
    let t = TableBuilder::new(2)
        .columns(vec![column("a", "A"), column("b", "B")])
        .head(row(vec![cell("A"), cell("B")]))
        .hide_column_labels()
        .title("T")
        .body(vec![row(vec![cell("x"), cell("y")])])
        .build();
    let src = render(&t, true);
    assert!(
        src.contains("\\toprule\n\\endfirsthead\n\\toprule\n\\endhead\n"),
        "{src}"
    );
}

/// A tabular's title and notes are paragraphs around it, as before.
#[test]
fn a_tabular_keeps_its_title_and_notes_outside() {
    let src = LatexWriter::new().render(&table(true, true)).unwrap();
    assert!(
        src.starts_with("{\\large\\bfseries T}\\\\\n{\\small S}\\\\[6pt]\n\\begin{tabular}{ll}"),
        "{src}"
    );
    assert!(
        src.ends_with("\\end{tabular}\n\n\\textsuperscript{a} F\\\\\n\n{\\footnotesize N}\\\\\n"),
        "{src}"
    );
}
