//! The XML parts of the minimal .docx package, other than the document.

use gridwell_core::xml::escape;
use gridwell_core::{ImageFormat, LanguageTag};

/// `[Content_Types].xml`, with a default for each of the `images`' formats.
pub fn content_types(images: &[ImageFormat]) -> String {
    let mut defaults = String::new();
    for (i, f) in images.iter().enumerate() {
        if !images[..i].contains(f) {
            defaults.push_str(&format!(
                "\n  <Default Extension=\"{}\" ContentType=\"{}\"/>",
                f.extension(),
                f.mime()
            ));
        }
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>{defaults}
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#
    )
}

pub const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

/// `word/_rels/document.xml.rels`: the styles, and each image (id, target).
pub fn document_rels(images: &[(String, &str)]) -> String {
    let mut rels = String::new();
    for (id, target) in images {
        rels.push_str(&format!(
            "\n  <Relationship Id=\"{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"{target}\"/>"
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>{rels}
</Relationships>"#
    )
}

/// Text sizes in half-points (`w:sz`): the document default (Word's 11pt), the
/// title, subtitle and header lines, and notes.
pub const BODY_HALF_POINTS: u32 = 22;
pub const TITLE_HALF_POINTS: u32 = 32;
pub const SUBTITLE_HALF_POINTS: u32 = 24;
pub const NOTE_HALF_POINTS: u32 = 18;

/// `word/styles.xml`: the document defaults (Calibri 11pt, no paragraph spacing,
/// the table's language) and every style the document names: `Title`, `Subtitle`, `FootnoteText` and
/// `SourceNote` paragraphs, and the `TableGrid` table style over `TableNormal`
/// (Word's cell margins). Word and LibreOffice treat a style that is named but not
/// defined as `Normal`.
pub fn styles(lang: Option<&LanguageTag>) -> String {
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
             <w:sz w:val=\"{BODY_HALF_POINTS}\"/><w:szCs w:val=\"{BODY_HALF_POINTS}\"/>{}\
             </w:rPr></w:rPrDefault><w:pPrDefault><w:pPr>\
             <w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/>\
             </w:pPr></w:pPrDefault></w:docDefaults>",
            lang.map(w_lang).unwrap_or_default()
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

/// `w:lang` for a language: Word keeps one for each kind of text, so the tag is
/// also the East Asian language for Chinese, Japanese and Korean, and the
/// complex-script one for a right-to-left language.
fn w_lang(tag: &LanguageTag) -> String {
    let t = escape(&tag.tag);
    let mut out = format!("<w:lang w:val=\"{t}\"");
    if matches!(tag.language.as_deref(), Some("zh" | "ja" | "ko")) {
        out.push_str(&format!(" w:eastAsia=\"{t}\""));
    }
    if tag.is_rtl() {
        out.push_str(&format!(" w:bidi=\"{t}\""));
    }
    out.push_str("/>");
    out
}

/// Default column width in dxa (twentieths of a point; 1.5 inches).
pub const DEFAULT_COL_WIDTH_DXA: u32 = 2160;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_images_the_package_parts_are_unchanged() {
        assert_eq!(
            content_types(&[]),
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#
        );
        assert_eq!(
            document_rels(&[]),
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#
        );
    }

    #[test]
    fn each_image_format_has_one_default() {
        let ct = content_types(&[ImageFormat::Png, ImageFormat::Gif, ImageFormat::Png]);
        assert_eq!(
            ct.matches("Extension=\"png\" ContentType=\"image/png\"")
                .count(),
            1
        );
        assert_eq!(
            ct.matches("Extension=\"gif\" ContentType=\"image/gif\"")
                .count(),
            1
        );
        let rels = document_rels(&[("rIdImage1".into(), "media/image1.png")]);
        assert!(rels.contains(
            "Id=\"rIdImage1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/image1.png\""
        ));
    }
}
