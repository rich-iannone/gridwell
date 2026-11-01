//! The table's language and direction are attributes of the `Table` block, which
//! Pandoc's writers read: HTML writes them out, LaTeX reads `lang`.

use gridwell_ir::TextDirection;
use gridwell_testkit::examples::directed;
use gridwell_writer_pandoc::render_pandoc;
use serde_json::{json, Value};

/// The `Table` block's key-value attributes.
fn table_attrs(dir: Option<TextDirection>, lang: Option<&str>) -> Value {
    let table: Value = serde_json::from_str(&render_pandoc(&directed(dir, lang)).unwrap()).unwrap();
    assert_eq!(table["t"], "Table");
    table["c"][0][2].clone()
}

#[test]
fn language_and_direction_are_the_tables_attributes() {
    assert_eq!(
        table_attrs(Some(TextDirection::Rtl), Some("ar-EG")),
        json!([["lang", "ar-EG"], ["dir", "rtl"]])
    );
    // Pandoc writes HTML's `auto` as it is.
    assert_eq!(
        table_attrs(Some(TextDirection::Auto), None),
        json!([["dir", "auto"]])
    );
    assert_eq!(
        table_attrs(Some(TextDirection::Ltr), None),
        json!([["dir", "ltr"]])
    );
    assert_eq!(table_attrs(None, Some("he")), json!([["lang", "he"]]));
    assert_eq!(table_attrs(None, None), json!([]));
    // An unknown direction (a validation warning) isn't written.
    assert_eq!(
        table_attrs(Some(TextDirection::parse("sideways")), Some("fr")),
        json!([["lang", "fr"]])
    );
}
