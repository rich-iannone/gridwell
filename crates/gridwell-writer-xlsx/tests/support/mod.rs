//! Reading a cell's value and number format back out of the XLSX parts.

use gridwell_ir::Table;
use gridwell_testkit::{row, TableBuilder};
use gridwell_writer_xlsx::XlsxWriter;

/// The value of attribute `name` in the start tag `tag`.
pub fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    Some(&tag[start..start + tag[start..].find('"')?])
}

/// The `<c>` element for `cell_ref`.
pub fn cell_xml<'a>(sheet: &'a str, cell_ref: &str) -> &'a str {
    let start = sheet
        .find(&format!("<c r=\"{cell_ref}\""))
        .unwrap_or_else(|| panic!("no {cell_ref} in {sheet}"));
    let rest = &sheet[start..];
    let open = rest.find('>').unwrap();
    if rest[..open].ends_with('/') {
        return &rest[..=open];
    }
    &rest[..rest.find("</c>").unwrap() + 4]
}

/// The number format code of a cell (`General` without one).
pub fn format_code(sheet: &str, styles: &str, cell_ref: &str) -> String {
    let c = cell_xml(sheet, cell_ref);
    let s: usize = attr(c, "s").map_or(0, |s| s.parse().unwrap());
    let xfs = &styles[styles.find("<cellXfs").unwrap()..];
    let xf = xfs.split("<xf ").nth(s + 1).unwrap();
    let id: u32 = attr(&format!(" {xf}"), "numFmtId")
        .unwrap()
        .parse()
        .unwrap();
    match id {
        0 => "General".into(),
        1 => "0".into(),
        2 => "0.00".into(),
        3 => "#,##0".into(),
        4 => "#,##0.00".into(),
        _ => {
            let fmt = styles
                .split("<numFmt ")
                .skip(1)
                .find(|f| attr(&format!(" {f}"), "numFmtId") == Some(&id.to_string()))
                .unwrap_or_else(|| panic!("numFmt {id} missing: {styles}"));
            attr(&format!(" {fmt}"), "formatCode")
                .unwrap()
                .replace("&quot;", "\"")
        }
    }
}

/// A table of one body row.
pub fn one_row(cells: Vec<gridwell_ir::Cell>) -> Table {
    TableBuilder::new(cells.len() as u32)
        .body(vec![row(cells)])
        .build()
}

/// The sheet, styles and shared-strings parts.
pub fn render(t: &Table) -> (String, String, String) {
    let w = XlsxWriter::new();
    (
        w.render_sheet_xml(t).unwrap(),
        w.render_styles_xml(t).unwrap(),
        w.render_shared_strings_xml(t).unwrap(),
    )
}
