use super::*;

fn raw(value: &str) -> ContentNode {
    ContentNode::Raw {
        format: "html".into(),
        value: value.into(),
    }
}

fn text(value: &str) -> ContentNode {
    ContentNode::Text {
        value: value.into(),
        href: None,
    }
}

/// Nodes in brief: text as is, a styled run `[text|declarations]`, a line
/// break `⏎`, an image `<img src alt width height>`.
fn show(nodes: &[ContentNode]) -> String {
    let mut out = String::new();
    for n in nodes {
        match n {
            ContentNode::Text { value, href: None } => out.push_str(value),
            ContentNode::Text {
                value,
                href: Some(href),
            } => out.push_str(&format!("[{value}|→{href}]")),
            ContentNode::StyledText {
                value,
                style_id,
                href,
            } => {
                let id = style_id.as_deref().unwrap_or("");
                let decls = id.strip_prefix(STYLE_PREFIX).unwrap_or(id);
                let to = href.as_deref().map(|h| format!("→{h}")).unwrap_or_default();
                out.push_str(&format!("[{value}|{decls}{to}]"));
            }
            ContentNode::LineBreak {} => out.push('⏎'),
            ContentNode::Image {
                src,
                alt,
                width,
                height,
            } => out.push_str(&format!("<img {src} {alt:?} {width:?} {height:?}>")),
            ContentNode::FootnoteMark { mark_text, .. } => out.push_str(&format!("^{mark_text}")),
            ContentNode::Raw { format, value } => out.push_str(&format!("{{{format}:{value}}}")),
            ContentNode::Unknown(_) => out.push('?'),
        }
    }
    out
}

fn lowered(html: &str) -> String {
    show(&lower(&[raw(html)]))
}

#[test]
fn inline_formatting_becomes_styled_runs() {
    assert_eq!(
        lowered("<b><i>rich</i></b>"),
        "[rich|font-weight:bold;font-style:italic;]"
    );
    assert_eq!(
        lowered("a <strong>b</strong> <em>c</em> d"),
        "a [b|font-weight:bold;] [c|font-style:italic;] d"
    );
    assert_eq!(
        lowered("<u>u</u><s>s</s>"),
        "[u|text-decoration:underline;][s|text-decoration:line-through;]"
    );
    assert_eq!(lowered("<code>x()</code>"), "[x()|font-family:monospace;]");
    assert_eq!(lowered("<small>s</small>"), "[s|font-size:smaller;]");
    assert_eq!(
        lowered("<mark>m</mark>"),
        "[m|color:black;background-color:yellow;]"
    );
    // Upper case tags, and the innermost decoration.
    assert_eq!(
        lowered("<U>a<DEL>b</DEL></U>"),
        "[a|text-decoration:underline;][b|text-decoration:line-through;]"
    );
}

#[test]
fn style_attributes_set_known_properties() {
    assert_eq!(
        lowered(
            r#"<span style="color: #FF0000; FONT-WEIGHT: 700 !important; margin: 4px">r</span>"#
        ),
        "[r|font-weight:700;color:#FF0000;]"
    );
    // A later declaration wins, and an element's own style is laid over its tag's.
    assert_eq!(
        lowered(r#"<b style="font-weight: normal; color: red; color: blue">n</b>"#),
        "[n|font-weight:normal;color:blue;]"
    );
    assert_eq!(
        lowered(r#"<font color="green" face="Courier New">f</font>"#),
        "[f|font-family:Courier New;color:green;]"
    );
    // Single-quoted and unquoted values; repeated attributes: the first.
    assert_eq!(
        lowered(
            "<span style='color:red' style='color:blue'>a</span><span style=color:navy>b</span>"
        ),
        "[a|color:red;][b|color:navy;]"
    );
}

#[test]
fn style_ids_resolve_to_their_declarations() {
    let nodes = lower(&[raw(
        r#"<span style="font-weight:bold;font-style:italic;color:#123456;text-decoration:underline dotted;font-size:12px;font-family:Georgia;background-color:yellow;text-transform:uppercase">x</span>"#,
    )])
    .into_owned();
    let [ContentNode::StyledText {
        style_id: Some(id), ..
    }] = &nodes[..]
    else {
        panic!("{nodes:?}");
    };
    let def = style_def(id).unwrap();
    assert!(def.font_weight.unwrap().is_bold());
    assert_eq!(def.font_style, Some(FontStyle::Italic));
    assert_eq!(def.color.as_deref(), Some("#123456"));
    assert_eq!(def.text_decoration, Some(TextDecoration::Underline));
    assert_eq!(def.font_size.as_deref(), Some("12px"));
    assert_eq!(def.font_family.as_deref(), Some("Georgia"));
    assert_eq!(def.background_color.as_deref(), Some("yellow"));
    assert_eq!(def.text_transform, Some(TextTransform::Uppercase));
    assert!(style_def("bold").is_none());
    // Not prefixed: an id of the table's own.
    assert!(style_def("html:font-weight:bold;").is_none());
}

#[test]
fn line_breaks_and_blocks() {
    assert_eq!(lowered("a<br>b<br/>c"), "a⏎b⏎c");
    assert_eq!(lowered("<p>one</p><p>two</p>"), "one⏎two");
    assert_eq!(lowered("<div>a</div>b<div>c</div>"), "a⏎b⏎c");
    // Edges of nested blocks break once; a block's edge at the start or end of
    // the markup doesn't break at all.
    assert_eq!(lowered("<div><p>a</p></div><div><p>b</p></div>"), "a⏎b");
    // `<br>` breaks even an empty line, but not the end.
    assert_eq!(lowered("a<br><br>b<br>"), "a⏎⏎b");
    assert_eq!(lowered("<ul><li>x</li><li>y</li></ul>"), "• x⏎• y");
    assert_eq!(lowered("<ol start=\"3\"><li>x<li>y</ol>"), "3. x⏎4. y");
    assert_eq!(
        lowered("<ol><li>a<ul><li>b</li></ul></li><li>c</li></ol>"),
        "1. a⏎• b⏎2. c"
    );
    assert_eq!(
        lowered("<table><tr><th>h</th><td>1</td></tr><tr><td>2</td><td>3</td></tr></table>"),
        "[h|font-weight:bold;] 1⏎2 3"
    );
}

#[test]
fn white_space_collapses_except_in_pre() {
    assert_eq!(lowered("  a \n\t b  "), "a b");
    assert_eq!(lowered("a <b> b </b> c"), "a [b |font-weight:bold;]c");
    assert_eq!(lowered("<p> a </p> <p> b </p>"), "a⏎b");
    assert_eq!(lowered("a&nbsp;&nbsp;b"), "a\u{a0}\u{a0}b");
    assert_eq!(
        lowered("<pre>\n  x  y\nz</pre>"),
        "[  x  y|font-family:monospace;]⏎[z|font-family:monospace;]"
    );
}

#[test]
fn superscripts_and_subscripts() {
    assert_eq!(lowered("m<sup>2</sup>"), "m²");
    assert_eq!(lowered("H<sub>2</sub>O"), "H₂O");
    assert_eq!(lowered("x<sup>n+1</sup>"), "xⁿ⁺¹");
    assert_eq!(
        lowered("e<sup>-iπ</sup>"),
        "e-iπ",
        "π has no superscript form"
    );
    assert_eq!(lowered("10<sup><b>3</b></sup>"), "10[³|font-weight:bold;]");
    // Unclosed, it still ends with the markup.
    assert_eq!(lowered("a<sup>2"), "a²");
    // `q` has no superscript form.
    assert_eq!(lowered("<sup>q</sup>"), "q");
}

#[test]
fn images_keep_their_attributes() {
    assert_eq!(
        lowered(r#"<img src="data:image/png;base64,AA==" alt="logo" width="32" height="2em">"#),
        r#"<img data:image/png;base64,AA== Some("logo") Some("32px") Some("2em")>"#
    );
    assert_eq!(
        lowered(r#"see <IMG SRC='https://example.com/x.png'> here"#),
        "see <img https://example.com/x.png None None None> here"
    );
}

#[test]
fn what_a_browser_hides_is_dropped() {
    assert_eq!(lowered("a<!-- note -->b"), "ab");
    assert_eq!(lowered("a<script>alert('<b>x</b>')</script>b"), "ab");
    assert_eq!(lowered("a<STYLE>b { color: red }</style>b"), "ab");
    assert_eq!(lowered("a<span hidden>x</span>b"), "ab");
    assert_eq!(
        lowered(r#"a<span style="display:none"><b>x</b><br></span>b"#),
        "ab"
    );
    assert_eq!(lowered("a<svg><text>x</text></svg>b"), "ab");
    assert_eq!(lowered("<!DOCTYPE html>a<?xml x?>b"), "ab");
}

#[test]
fn links_link_their_text() {
    assert_eq!(
        lowered(r#"see <a href="https://x.org/?a=1&amp;b=2">the <b>docs</b></a>."#),
        "see [the |→https://x.org/?a=1&b=2][docs|font-weight:bold;→https://x.org/?a=1&b=2]."
    );
    // Attribute white space is trimmed, as a browser does; the scheme is
    // matched ignoring case.
    assert_eq!(
        lowered(r#"<A HREF=" MAILTO:a@b.c ">mail</A>"#),
        "[mail|→MAILTO:a@b.c]"
    );
    // A run ends where its link does.
    assert_eq!(
        lowered(r#"<a href="https://a.org">a</a><a href="https://b.org">b</a>c"#),
        "[a|→https://a.org][b|→https://b.org]c"
    );
    // The innermost `a` wins; one without a link links nothing.
    assert_eq!(
        lowered(r#"<a href="https://a.org">a<a href="https://b.org">b</a><a>c</a></a>"#),
        "[a|→https://a.org][b|→https://b.org]c"
    );
    // Only http, https and mailto: the text of any other stays as it is.
    for bad in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "java&#9;script:alert(1)",
        "&#106;avascript:alert(1)",
        "data:text/html,x",
        "/relative",
        "#anchor",
        "",
    ] {
        assert_eq!(lowered(&format!(r#"<a href="{bad}">t</a>"#)), "t", "{bad}");
    }
    assert_eq!(lowered("<a>t</a>"), "t");
}

#[test]
fn everything_else_keeps_its_text() {
    assert_eq!(lowered("<custom-tag x=1>t</custom-tag>"), "t");
    assert_eq!(lowered("a < b > c"), "a < b > c");
    assert_eq!(lowered("<q>quoted</q>"), "\u{201C}quoted\u{201D}");
    // Stray and mismatched end tags.
    assert_eq!(lowered("a</b>b</i>"), "ab");
    assert_eq!(
        lowered("<b>a<i>b</b>c"),
        "[a|font-weight:bold;][b|font-weight:bold;font-style:italic;]c"
    );
}

#[test]
fn character_references() {
    assert_eq!(lowered("&lt;b&gt; &amp; &quot;&apos;"), "<b> & \"'");
    assert_eq!(
        lowered("&#169;&#xA9;&#XA9;&copy;&eacute;&Eacute;&yuml;"),
        "©©©©éÉÿ"
    );
    assert_eq!(lowered("&alpha;&Omega;&sigmaf;&Sigma;&Rho;"), "αΩςΣΡ");
    assert_eq!(lowered("&mdash;&hellip;&euro;&minus;&le;"), "—…€−≤");
    // Unknown, unterminated or out of range.
    assert_eq!(
        lowered("&bogus; &amp &Sigmaf; &aLPHA;"),
        "&bogus; &amp &Sigmaf; &aLPHA;"
    );
    assert_eq!(
        lowered("&#0;&#x110000;&#99999999999;&#xD800;"),
        "\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}"
    );
    assert_eq!(lowered("&#65"), "A");
    assert_eq!(lowered("&#;&#x;"), "&#;&#x;");
    // In attribute values too.
    assert_eq!(
        lowered(r#"<img src="a&amp;b" alt="&lt;3">"#),
        r#"<img a&b Some("<3") None None>"#
    );
}

#[test]
fn entity_tables_line_up_with_unicode() {
    assert_eq!(named_entity("nbsp"), Some('\u{A0}'));
    assert_eq!(named_entity("frac34"), Some('¾'));
    assert_eq!(named_entity("ETH"), Some('Ð'));
    assert_eq!(named_entity("times"), Some('×'));
    assert_eq!(named_entity("divide"), Some('÷'));
    assert_eq!(named_entity("yuml"), Some('ÿ'));
    assert_eq!(named_entity("Alpha"), Some('Α'));
    assert_eq!(named_entity("rho"), Some('ρ'));
    assert_eq!(named_entity("tau"), Some('τ'));
    assert_eq!(named_entity("Tau"), Some('Τ'));
    assert_eq!(named_entity("omega"), Some('ω'));
}

#[test]
fn context_around_the_markup() {
    // A space collapses with the text before; a block edge before more content
    // breaks the line.
    let nodes = [text("a "), raw(" <b>b</b>"), raw("<p>c</p>"), text("d")];
    assert_eq!(show(&lower(&nodes)), "a [b|font-weight:bold;]⏎c⏎d");
    // A final break or space is dropped only at the end of a line.
    let nodes = [raw("x<br>"), text("y")];
    assert_eq!(show(&lower(&nodes)), "x⏎y");
    let nodes = [raw("x "), text("y")];
    assert_eq!(show(&lower(&nodes)), "x y");
    let nodes = [raw("x "), ContentNode::LineBreak {}, raw(" y")];
    assert_eq!(show(&lower(&nodes)), "x⏎y");
}

#[test]
fn content_without_html_is_borrowed() {
    let nodes = [text("a")];
    assert!(matches!(lower(&nodes), Cow::Borrowed(_)));
    let latex = ContentNode::Raw {
        format: "latex".into(),
        value: "\\textbf{x}".into(),
    };
    let nodes = [latex.clone()];
    assert!(matches!(lower(&nodes), Cow::Borrowed(_)));
    // Versions for several formats: each writer takes its own.
    let nodes = [raw("<b>x</b>"), latex];
    assert!(matches!(lower(&nodes), Cow::Borrowed(_)));
    // The format's name in any case.
    let nodes = [ContentNode::Raw {
        format: "HTML".into(),
        value: "<i>x</i>".into(),
    }];
    assert_eq!(show(&lower(&nodes)), "[x|font-style:italic;]");
}

#[test]
fn malformed_markup_never_panics() {
    let inputs = [
        "<",
        "<b",
        "<b ",
        "<b x",
        "<b x=",
        "<b x=\"",
        "<b x='y",
        "</",
        "</b",
        "<!",
        "<!-",
        "<!--",
        "<!-- x",
        "<?",
        "&",
        "&#",
        "&#x",
        "&#x1F600",
        "<img",
        "<img src=",
        "<b =x>t",
        "<b/=/>t",
        "<br/",
        "<sup>",
        "</sup>",
        "<sup></sup>",
        "<script>",
        "<script>x</scr",
        "<p><p><p>",
        "<li>",
        "</li></ol>",
        "<ol start=-5><li>a",
        "<ol start=99999999999><li>a",
        "é<b>ü</b>&eacute",
        "<b>\u{0}</b>",
        "<span style=';;:;color:;:red'>x</span>",
        "<pre>",
        "<pre>\r\n\r\nx",
        "<h7>x</h7>",
        "<h1>x",
        "<<>>",
        "<a<b>>",
    ];
    for input in inputs {
        let _ = lower(&[raw(input)]);
        let _ = lower(&[
            text("t "),
            raw(input),
            raw(input),
            ContentNode::LineBreak {},
        ]);
    }
    // Every prefix of a long, mixed document, cut at every character boundary.
    let doc = r#"<div class="x"><p>A&amp;B <b style="color:red">bold <i>both</i></b><br>
        <img src="data:image/png;base64,AA==" alt="&lt;i&gt;" width=3>
        <ul><li>one<li>two<sup>2</sup></ul><!-- c --><script>x</script></p></div>"#;
    for (i, _) in doc.char_indices() {
        let _ = lower(&[raw(&doc[..i])]);
        let _ = lower(&[raw(&doc[i..])]);
    }
}

#[test]
fn deep_nesting_and_long_input_stay_linear() {
    let deep = format!("{}x{}", "<b><i>".repeat(5000), "</i></b>".repeat(5000));
    assert_eq!(lowered(&deep), "[x|font-weight:bold;font-style:italic;]");
    let deep_lists = "<ol><li>".repeat(5000);
    let _ = lowered(&deep_lists);
    let attrs = format!("<b {}>x</b>", "a=1 ".repeat(20000));
    assert_eq!(lowered(&attrs), "[x|font-weight:bold;]");
    let stray = "</b>".repeat(20000) + "x";
    assert_eq!(lowered(&stray), "x");
    let start = std::time::Instant::now();
    let long = "<span style=\"color:red\">x</span> &amp; <b>y</b><br>".repeat(20000);
    let _ = lowered(&long);
    assert!(start.elapsed().as_secs() < 5, "{:?}", start.elapsed());
}

fn raw_in(format: &str, value: &str) -> ContentNode {
    ContentNode::Raw {
        format: format.into(),
        value: value.into(),
    }
}

/// A one-cell table with `content`, and a title holding `title`.
fn one_cell(content: Vec<ContentNode>, title: Vec<ContentNode>) -> Table {
    use gridwell_testkit::{row, TableBuilder};
    let cell = gridwell_ir::Cell {
        content,
        ..gridwell_testkit::cell("")
    };
    let mut t = TableBuilder::new(1)
        .title("t")
        .body(vec![row(vec![cell])])
        .build();
    t.header.as_mut().unwrap().title.as_mut().unwrap().content = title;
    t
}

fn body_cell(t: &Table) -> &[ContentNode] {
    &t.table.tbody[0].rows[0].cells[0].content
}

#[test]
fn a_table_without_raw_nodes_is_borrowed() {
    let t = one_cell(vec![text("a")], vec![text("b")]);
    assert!(matches!(without_raw(&t), Cow::Borrowed(_)));
}

#[test]
fn without_raw_lowers_html_and_drops_the_rest() {
    let t = one_cell(
        // Versions for two formats: HTML's is drawn, as every format draws it.
        vec![
            raw("<i>x</i><script>s</script>"),
            raw_in("latex", "\\textit{x}"),
        ],
        vec![
            text("a "),
            raw_in("typst", "#read(\"f\")"),
            raw_in("rtf", "{\\field}"),
            raw_in("HTML", "<b>b</b>"),
        ],
    );
    let out = without_raw(&t);
    assert_eq!(show(body_cell(&out)), "[x|font-style:italic;]");
    let title = &out.header.as_ref().unwrap().title.as_ref().unwrap().content;
    assert_eq!(show(title), "a [b|font-weight:bold;]");
    // What isn't content is left as it is.
    let rest = |t: &Table| {
        let mut v = serde_json::to_value(t).unwrap();
        v["header"]["title"]["content"].take();
        v["table"]["tbody"][0]["rows"][0]["cells"][0]["content"].take();
        v
    };
    assert_eq!(rest(&out), rest(&t));
    // Only raw markup for other formats: nothing is left of it.
    let t = one_cell(vec![text("a"), raw_in("latex", "x")], vec![]);
    assert_eq!(show(body_cell(&without_raw(&t))), "a");
}
