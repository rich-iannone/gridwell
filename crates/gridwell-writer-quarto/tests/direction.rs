//! The table's language and direction are attributes of the Quarto `Div`, so the
//! notes after the table have them too, as well as of the `Table` inside it.

use gridwell_ir::TextDirection;
use gridwell_testkit::examples::directed;
use gridwell_writer_quarto::render_quarto;
use serde_json::{json, Value};

/// The `Div`'s and its `Table`'s key-value attributes.
fn attrs(dir: Option<TextDirection>, lang: Option<&str>) -> (Value, Value) {
    let div: Value = serde_json::from_str(&render_quarto(&directed(dir, lang)).unwrap()).unwrap();
    assert_eq!(div["t"], "Div");
    let table = div["c"][1]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["t"] == "Table")
        .unwrap();
    (div["c"][0][2].clone(), table["c"][0][2].clone())
}

#[test]
fn language_and_direction_are_the_divs_and_the_tables() {
    let (div, table) = attrs(Some(TextDirection::Rtl), Some("ar"));
    assert_eq!(table, json!([["lang", "ar"], ["dir", "rtl"]]));
    assert_eq!(
        div,
        json!([["tbl-cap", "Title"], ["lang", "ar"], ["dir", "rtl"]])
    );
    let (div, table) = attrs(None, None);
    assert_eq!(table, json!([]));
    assert_eq!(div, json!([["tbl-cap", "Title"]]));
    let (div, _) = attrs(Some(TextDirection::parse("sideways")), None);
    assert_eq!(div, json!([["tbl-cap", "Title"]]));
}
