//! Static XML parts for the minimal .docx package.

pub const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#;

pub const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

pub const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;

/// Text sizes in half-points (`w:sz`): the document default (Word's 11pt), the
/// title, subtitle and header lines, and notes.
pub const BODY_HALF_POINTS: u32 = 22;
pub const TITLE_HALF_POINTS: u32 = 32;
pub const SUBTITLE_HALF_POINTS: u32 = 24;
pub const NOTE_HALF_POINTS: u32 = 18;

/// `word/styles.xml`: the document defaults (Calibri 11pt, no paragraph spacing)
/// and every style the document names: `Title`, `Subtitle`, `FootnoteText` and
/// `SourceNote` paragraphs, and the `TableGrid` table style over `TableNormal`
/// (Word's cell margins). Word and LibreOffice treat a style that is named but not
/// defined as `Normal`.
pub fn styles() -> String {
    let size = |hp: u32| format!("<w:rPr><w:sz w:val=\"{hp}\"/><w:szCs w:val=\"{hp}\"/></w:rPr>");
    let line = |side: &str| {
        format!("<w:{side} w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>")
    };
    let grid: String = ["top", "left", "bottom", "right", "insideH", "insideV"]
        .into_iter()
        .map(line)
        .collect();
    let margin = |side: &str, w: u32| format!("<w:{side} w:w=\"{w}\" w:type=\"dxa\"/>");
    let margins = [("top", 0), ("left", 108), ("bottom", 0), ("right", 108)]
        .into_iter()
        .map(|(side, w)| margin(side, w))
        .collect::<String>();
    [
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#.to_string(),
        r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#
            .to_string(),
        format!(
            "<w:docDefaults><w:rPrDefault><w:rPr>\
             <w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:cs=\"Calibri\"/>\
             <w:sz w:val=\"{BODY_HALF_POINTS}\"/><w:szCs w:val=\"{BODY_HALF_POINTS}\"/>\
             </w:rPr></w:rPrDefault><w:pPrDefault><w:pPr>\
             <w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/>\
             </w:pPr></w:pPrDefault></w:docDefaults>"
        ),
        r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>"#
            .to_string(),
        // The title and its lines keep with the table below them.
        format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"Title\"><w:name w:val=\"Title\"/>\
             <w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>\
             <w:pPr><w:keepNext/></w:pPr>{}</w:style>",
            size(TITLE_HALF_POINTS)
        ),
        format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"Subtitle\"><w:name w:val=\"Subtitle\"/>\
             <w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>\
             <w:pPr><w:keepNext/></w:pPr>{}</w:style>",
            size(SUBTITLE_HALF_POINTS)
        ),
        format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"FootnoteText\">\
             <w:name w:val=\"footnote text\"/><w:basedOn w:val=\"Normal\"/>{}</w:style>",
            size(NOTE_HALF_POINTS)
        ),
        r#"<w:style w:type="paragraph" w:customStyle="1" w:styleId="SourceNote"><w:name w:val="Source Note"/><w:basedOn w:val="FootnoteText"/><w:qFormat/></w:style>"#
            .to_string(),
        format!(
            "<w:style w:type=\"table\" w:default=\"1\" w:styleId=\"TableNormal\">\
             <w:name w:val=\"Normal Table\"/><w:uiPriority w:val=\"99\"/><w:semiHidden/>\
             <w:unhideWhenUsed/><w:tblPr><w:tblInd w:w=\"0\" w:type=\"dxa\"/>\
             <w:tblCellMar>{margins}</w:tblCellMar></w:tblPr></w:style>"
        ),
        format!(
            "<w:style w:type=\"table\" w:styleId=\"TableGrid\"><w:name w:val=\"Table Grid\"/>\
             <w:basedOn w:val=\"TableNormal\"/><w:tblPr><w:tblBorders>{grid}</w:tblBorders>\
             </w:tblPr></w:style>"
        ),
        "</w:styles>".to_string(),
    ]
    .join("\n")
}

/// Default column width in dxa (twentieths of a point; 1.5 inches).
pub const DEFAULT_COL_WIDTH_DXA: u32 = 2160;
