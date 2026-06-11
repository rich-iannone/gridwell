//! Static XML parts for the minimal .docx package.

pub const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;

pub const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

pub const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
</Relationships>"#;

// OOXML table widths use "dxa" (twentieths of a point): 1 inch = 1440 dxa.
const DXA_PER_PX: f64 = 15.0; // 1px ≈ 15 dxa at 96dpi

pub fn px_to_dxa(px: f64) -> u32 {
    (px * DXA_PER_PX) as u32
}

/// Default column width in dxa (1.5 inches).
pub const DEFAULT_COL_WIDTH_DXA: u32 = 2160;

/// Convert a hex color like "#FF0000" to OOXML color format "FF0000".
pub fn hex_to_ooxml_color(hex: &str) -> Option<String> {
    let h = hex.strip_prefix('#')?;
    if h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(h.to_uppercase())
    } else {
        None
    }
}
