//! A format is in the registry, and its writer in the crate, exactly when its
//! feature is on. CI runs this with every feature, with each one alone and with
//! none.

use gridwell::{RenderError, Table};

/// Every format with whether its feature is on, in registry order.
const FORMATS: [(&str, bool); 11] = [
    ("html", cfg!(feature = "html")),
    ("latex", cfg!(feature = "latex")),
    ("typst", cfg!(feature = "typst")),
    ("rtf", cfg!(feature = "rtf")),
    ("svg", cfg!(feature = "svg")),
    ("ansi", cfg!(feature = "ansi")),
    ("pandoc", cfg!(feature = "pandoc")),
    ("quarto", cfg!(feature = "quarto")),
    ("docx", cfg!(feature = "docx")),
    ("xlsx", cfg!(feature = "xlsx")),
    ("pptx", cfg!(feature = "pptx")),
];

fn table() -> Table {
    Table::from_json(
        r#"{"ir_version":"1.0","config":{"table_cols":1,"body_rows":1},
            "styles":{"defs":{},"compositions":{},"conditionals":[]},
            "column_spec":[{"id":"a"}],
            "table":{"thead":{"rows":[]},"tbody":[{"rows":[{"cells":[
              {"content":[{"type":"text","value":"x"}]}]}]}]}}"#,
    )
    .unwrap()
}

#[test]
fn the_registry_has_the_formats_whose_features_are_on() {
    let on: Vec<&str> = FORMATS.iter().filter(|f| f.1).map(|f| f.0).collect();
    assert_eq!(gridwell::names(), on);
}

#[test]
fn a_format_whose_feature_is_off_is_unknown() {
    for (name, on) in FORMATS {
        match gridwell::find(name) {
            Ok(_) => assert!(on, "{name} is registered with its feature off"),
            Err(RenderError::UnknownFormat { name: n }) => {
                assert!(!on, "{name} isn't registered with its feature on");
                assert_eq!(n, name);
            }
            Err(e) => panic!("{name}: {e}"),
        }
        let rendered = gridwell::render(&table(), name, None);
        assert_eq!(rendered.is_ok(), on, "{name}");
    }
}

/// Each writer that is on is in the crate, under its format's name, and writes
/// what the registry does with the default options.
#[test]
// With no format on, nothing here is used.
#[allow(unused_mut, unused_variables)]
fn each_writer_is_in_the_crate_with_its_feature() {
    let table = table();
    let mut found: Vec<&str> = Vec::new();
    macro_rules! writer {
        ($feature:literal, $render:path) => {
            #[cfg(feature = $feature)]
            {
                let direct: Vec<u8> = $render(&table).unwrap().into();
                let registry = gridwell::render(&table, $feature, None).unwrap();
                assert_eq!(direct, registry.into_bytes(), $feature);
                found.push($feature);
            }
        };
    }
    writer!("html", gridwell::html::render_html);
    writer!("latex", gridwell::latex::render_latex);
    writer!("typst", gridwell::typst::render_typst);
    writer!("rtf", gridwell::rtf::render_rtf);
    writer!("svg", gridwell::svg::render_svg);
    writer!("ansi", gridwell::ansi::render_ansi);
    writer!("pandoc", gridwell::pandoc::render_pandoc);
    writer!("quarto", gridwell::quarto::render_quarto);
    writer!("docx", gridwell::docx::render_docx);
    writer!("xlsx", gridwell::xlsx::render_xlsx);
    writer!("pptx", gridwell::pptx::render_pptx);
    assert_eq!(found, gridwell::names());
}

#[test]
fn notebooks_get_html_with_the_html_feature() {
    #[cfg(feature = "html")]
    assert!(gridwell::display_html(&table())
        .unwrap()
        .contains("data-quarto-disable-processing"));
    // Without it there is no display HTML to call; the rest still works.
    assert!(gridwell::describe(&table()).starts_with("1 column, 1 row"));
}
