//! Colours, lengths and font values reach CSS only in normalized form: hostile IR
//! strings can't break out of a `style` attribute or the `<style>` block, and valid
//! values in any accepted form come out as well-formed CSS.

use gridwell_ir::Table;
use gridwell_writer_html::{HtmlWriter, HtmlWriterConfig};
use serde_json::{json, Value};

const PAYLOAD: &str = "1px; evil: 1\"><script>alert(1)</script></style><b>{}";

/// A table with `v` in every colour, length and font field.
fn table_with(v: &str) -> Table {
    let style = json!({
        "font_family": v, "font_size": v, "color": v, "background_color": v, "indent": v,
        "min_width": v, "max_width": v,
        "padding": { "top": v, "right": v, "bottom": v, "left": v },
        "border": { "top": { "width": v, "style": "solid", "color": v } }
    });
    let ir: Value = json!({
        "ir_version": "1.0",
        "config": { "table_cols": 1, "header_rows": 0, "body_rows": 1,
                    "table_width": v, "container_width": v, "container_height": v },
        "styles": { "defs": { "s": style }, "compositions": {}, "conditionals": [] },
        "column_spec": [{ "id": "a", "width": v, "min_width": v, "max_width": v }],
        "table": {
            "thead": { "rows": [] },
            "tbody": [{ "rows": [{ "cells": [{
                "style_id": "s",
                "content": [
                    { "type": "styled_text", "value": "x", "style_id": "s" },
                    { "type": "image", "src": "a.png", "width": v, "height": v }
                ]
            }] }] }]
        }
    });
    Table::from_json(&ir.to_string()).unwrap()
}

fn render(table: &Table, inline: bool) -> String {
    HtmlWriter::with_config(HtmlWriterConfig {
        inline_styles: inline,
        ..Default::default()
    })
    .render(table)
    .unwrap()
}

#[test]
fn hostile_values_never_reach_the_output() {
    for inline in [false, true] {
        let html = render(&table_with(PAYLOAD), inline);
        for bad in ["<script", "evil:", "</style><b>", "\"><", "{}"] {
            assert!(
                !html.contains(bad),
                "inline={inline}: {bad:?} leaked into:\n{html}"
            );
        }
    }
}

#[test]
fn colours_are_normalized() {
    for (input, css) in [
        ("hsl(0 100% 50%)", "#FF0000"),
        ("rebeccapurple", "#663399"),
        ("#abc", "#AABBCC"),
        ("rgb(0 0 255 / 50%)", "rgba(0, 0, 255, 0.502)"),
        ("transparent", "transparent"),
    ] {
        let mut t = table_with("0");
        let s = t.styles.defs.get_mut("s").unwrap();
        s.color = Some(input.into());
        s.background_color = Some(input.into());
        for inline in [false, true] {
            let html = render(&t, inline);
            assert!(html.contains(&format!("color: {css}")), "{input}: {html}");
            assert!(
                html.contains(&format!("background-color: {css}")),
                "{input}: {html}"
            );
        }
    }
}

#[test]
fn lengths_are_normalized_and_bad_ones_dropped() {
    let mut t = table_with("0");
    t.config.table_width = Some(" 100% ".into());
    t.config.container_width = Some("900PX".into());
    t.config.container_height = Some("12 px".into());
    t.column_spec[0].width = "2.50EM".into();
    let html = render(&t, true);
    assert!(html.contains("style=\"width: 100%\""), "{html}");
    assert!(html.contains("max-width: 900px"), "{html}");
    assert!(!html.contains("max-height"), "{html}");
    assert!(html.contains("<col style=\"width: 2.5em\">"), "{html}");
}

#[test]
fn font_sizes_and_families() {
    let mut t = table_with("0");
    let s = t.styles.defs.get_mut("s").unwrap();
    s.font_size = Some("X-Large".into());
    s.font_family = Some("\"Helvetica Neue\", Arial, sans-serif".into());
    let html = render(&t, false);
    assert!(html.contains("font-size: x-large"), "{html}");
    assert!(
        html.contains("font-family: \"Helvetica Neue\", Arial, sans-serif"),
        "{html}"
    );
    // In a style attribute the quotes are entity-escaped, not dropped.
    let html = render(&t, true);
    assert!(
        html.contains("font-family: &quot;Helvetica Neue&quot;, Arial, sans-serif"),
        "{html}"
    );
}

#[test]
fn unknown_keywords_never_reach_the_output() {
    let mut t = table_with("0");
    let set = |v: &mut Value| {
        for f in [
            "font_weight",
            "font_style",
            "text_align",
            "vertical_align",
            "text_transform",
            "text_decoration",
            "white_space",
            "word_break",
            "overflow",
            "text_overflow",
        ] {
            v["styles"]["defs"]["s"][f] = json!(PAYLOAD);
        }
        v["styles"]["defs"]["s"]["border"]["top"]["style"] = json!(PAYLOAD);
        v["config"]["container_overflow"] = json!(PAYLOAD);
        v["config"]["header_rows"] = json!(1);
        v["table"]["thead"]["rows"] = json!([{ "cells": [
            { "content": [{ "type": "text", "value": "h" }], "scope": PAYLOAD }
        ] }]);
    };
    let mut v: Value = serde_json::from_str(&t.to_json().unwrap()).unwrap();
    set(&mut v);
    t = Table::from_json(&v.to_string()).unwrap();
    for inline in [false, true] {
        let html = render(&t, inline);
        for bad in ["<script", "evil:", "</style><b>"] {
            assert!(
                !html.contains(bad),
                "inline={inline}: {bad:?} leaked:\n{html}"
            );
        }
    }
}
