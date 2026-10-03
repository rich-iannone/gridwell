//! Property tests over arbitrary valid IR (`gridwell_testkit::arb::arb_valid_ir`):
//! every format renders without error, and every format that can be read back
//! shows exactly the IR's grid.

mod support;

use gridwell_render::{names, render};
use gridwell_testkit::arb::arb_valid_ir;
use proptest::prelude::*;
use support::readers::{html_grid, pandoc_grid, rtf_grid, xlsx_grid};
use support::{compare, expected, ooxml};

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        ..ProptestConfig::default()
    })]

    #[test]
    fn every_format_renders_and_readable_formats_show_the_grid(t in arb_valid_ir()) {
        for name in names() {
            prop_assert!(render(&t, name, None).is_ok(), "{name} failed");
        }
        let want = expected(&t);
        let text = |f: &str| render(&t, f, None).unwrap().as_text().unwrap().to_string();
        let mut checks = vec![
            ("html", Some(html_grid(&text("html"))), false),
            ("pandoc", Some(pandoc_grid(&text("pandoc"))), false),
            ("quarto", Some(pandoc_grid(&text("quarto"))), false),
            ("rtf", rtf_grid(&text("rtf"), want.width()), false),
        ];
        let docx = gridwell_writer_docx::DocxWriter::new().render_document_xml(&t).unwrap();
        checks.push(("docx", Some(ooxml::docx_grid(&docx)), false));
        let pptx = gridwell_writer_pptx::PptxWriter::new().render_slide_xml(&t).unwrap();
        checks.push(("pptx", Some(ooxml::pptx_grid(&pptx)), false));
        let xlsx = render(&t, "xlsx", None).unwrap().into_bytes();
        checks.push(("xlsx", Some(xlsx_grid(&xlsx, want.header_lines, want.rows.len(), want.width())), true));
        for (format, got, numbers) in checks {
            // RTF can't always tell columns apart (see `rtf_grid`).
            let Some(got) = got else { continue };
            if let Err(e) = compare(&got, &want, numbers) {
                prop_assert!(false, "{format}: {e}\nIR: {}", t.to_json().unwrap());
            }
        }
    }
}
