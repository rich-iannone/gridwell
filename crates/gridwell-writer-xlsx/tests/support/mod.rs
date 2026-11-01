//! Reading a cell's value, text and number format back out of the XLSX parts.

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

/// The shared-string item (`<si>` content) of the string cell `cell_ref`.
pub fn string_item<'a>(sheet: &str, sst: &'a str, cell_ref: &str) -> &'a str {
    let c = cell_xml(sheet, cell_ref);
    assert_eq!(attr(c, "t"), Some("s"), "not a string: {c}");
    let v = &c[c.find("<v>").unwrap() + 3..];
    let i: usize = v[..v.find("</v>").unwrap()].parse().unwrap();
    let item = sst
        .split("<si>")
        .nth(i + 1)
        .unwrap_or_else(|| panic!("no string {i}: {sst}"));
    &item[..item.find("</si>").unwrap()]
}

/// The text of the string cell `cell_ref`: its runs' `<t>`s, unescaped.
pub fn text_of(sheet: &str, sst: &str, cell_ref: &str) -> String {
    let item = string_item(sheet, sst, cell_ref);
    item.split("<t")
        .skip(1)
        .map(|t| &t[t.find('>').unwrap() + 1..t.find("</t>").unwrap()])
        .collect::<String>()
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// The reference of the string cell whose text is `text`.
pub fn find_text(sheet: &str, sst: &str, text: &str) -> String {
    sheet
        .split("<c r=\"")
        .skip(1)
        .map(|c| &c[..c.find('"').unwrap()])
        .find(|r| attr(cell_xml(sheet, r), "t") == Some("s") && text_of(sheet, sst, r) == text)
        .unwrap_or_else(|| panic!("no cell shows {text:?}: {sheet}"))
        .to_owned()
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
