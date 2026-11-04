"""The Great Tables emitter's styles and footnote marks: `tab_style()` per cell,
footnote marks in cells, and the table options' styles, against what Great Tables
draws (its HTML)."""

import json
from html.parser import HTMLParser

import pytest

pd = pytest.importorskip("pandas")
great_tables = pytest.importorskip("great_tables")
from great_tables import GT, exibble, loc, style

import gridwell
from gridwell._gt_emitter import _css_style

_locations = pytest.importorskip("great_tables._locations")
if not hasattr(_locations, "LocGrandSummaryStub"):
    pytest.skip("Great Tables before 1.0 has no summary locations", allow_module_level=True)


def mark_tokens(nodes):
    """Content as its text pieces and marks ("^" + the marks, those next to each
    other together, as Great Tables draws them), white space between them dropped."""
    out = []
    for n in nodes:
        if n["type"] == "footnote_mark":
            if out and out[-1].startswith("^") and out[-1].endswith(","):
                out[-1] += n["mark_text"]
            else:
                out.append("^" + n["mark_text"])
        elif n.get("value", "").strip():
            out.append(n["value"].strip())
    return out


class _Cells(HTMLParser):
    """Great Tables' HTML as its cells, in document order: (section, classes,
    style attribute, text pieces and marks)."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.cells, self.section, self.cell, self.in_mark = [], None, None, False

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag in ("thead", "tbody", "tfoot"):
            self.section = tag
        elif tag in ("th", "td"):
            self.cell = (self.section, set((a.get("class") or "").split()), a.get("style") or "", [])
            self.cells.append(self.cell)
        elif tag == "span" and "gt_footnote_marks" in (a.get("class") or ""):
            self.in_mark = True

    def handle_endtag(self, tag):
        if tag == "span":
            self.in_mark = False

    def handle_data(self, data):
        if self.cell is not None and data.strip():
            self.cell[3].append(("^" + data.strip()) if self.in_mark else data.strip())


def drawn(gt_obj):
    """What Great Tables draws, by part: each cell's text and marks with its style
    attribute read as the IR's style definitions."""
    parser = _Cells()
    parser.feed(gt_obj.as_raw_html())
    parts = {"head": {}, "body": [], "footnotes": [], "source_notes": []}
    for section, classes, css, tokens in parser.cells:
        entry = (tokens, _css_style(css))
        if "gt_title" in classes:
            parts["title"] = entry
        elif "gt_subtitle" in classes:
            parts["subtitle"] = entry
        elif section == "thead":
            if tokens:
                parts["head"][tokens[0]] = entry
        elif "gt_footnote" in classes:
            # The footnote's own mark comes first; the IR has it as `mark`.
            parts["footnotes"].append((tokens[1:], entry[1]))
        elif "gt_sourcenote" in classes:
            parts["source_notes"].append(entry)
        elif section == "tbody":
            parts["body"].append(entry)
    return parts


def emitted(gt_obj):
    """The same parts of gridwell's IR for the table (only the cells' own styles)."""
    ir = gridwell.gt_to_dict(gt_obj)
    defs = ir["styles"]["defs"]

    def entry(part):
        return (mark_tokens(part["content"]), defs.get(part.get("style_id"), {}))

    parts = {
        **{k: entry(ir["header"][k]) for k in ("title", "subtitle") if ir["header"][k]},
        "head": {},
        "body": [],
        "footnotes": [entry(f) for f in ir["footer"]["footnotes"]],
        "source_notes": [entry(n) for n in ir["footer"]["source_notes"]],
    }
    for row in ir["table"]["thead"]["rows"]:
        for c in row["cells"]:
            if c["content"]:
                parts["head"][mark_tokens(c["content"])[0]] = entry(c)
    for group in ir["table"]["tbody"]:
        if group.get("label"):
            parts["body"].append(entry(group["label"]))
        for row in group["rows"] + group["summary_rows"]:
            parts["body"].extend(entry(c) for c in row["cells"])
    return ir, parts


def text_color(c):
    return style.text(color=c)


def everywhere():
    """A style and a footnote at every kind of location Great Tables has."""
    df = pd.DataFrame({
        "r": ["r1", "r2", "r3"], "g": ["G1", "G1", "G2"],
        "a": [1, 2, 3], "b": [4.5, 5.5, 6.5], "c": ["x", "y", "z"], "d": [7, 8, 9],
    })
    return (
        GT(df, rowname_col="r", groupname_col="g")
        .tab_header(title="Title", subtitle="Sub")
        .tab_stubhead("Stubhead")
        .tab_spanner("Low", ["a", "b"])
        .tab_spanner("High", ["a", "b", "c"])
        .summary_rows(fns={"Max": lambda x: x[["a", "b", "d"]].max()})
        .grand_summary_rows(fns={"Sum": lambda x: x[["a", "b", "d"]].sum()})
        .tab_source_note("Source.")
        .tab_style(text_color("#000001"), loc.header())
        .tab_style([style.fill("#EEEEE1"), style.text(weight="bold")], loc.title())
        .tab_style(style.text(style="italic", size="12px"), loc.subtitle())
        .tab_style(style.fill("#EEEEE2"), loc.stubhead())
        .tab_style(style.borders(sides="bottom", color="#123456", weight=3), loc.column_header())
        .tab_style(text_color("#000002"), loc.column_labels(columns=["a", "d"]))
        .tab_style(text_color("#000003"), loc.spanner_labels(ids=["Low"]))
        .tab_style(style.fill("#EEEEE3"), loc.row_groups(rows=["G2"]))
        .tab_style(text_color("#000004"), loc.stub(rows=[1]))
        .tab_style(
            [style.fill("#EEEEE4"), style.text(font="Courier New, monospace", align="right")],
            loc.body(columns="c", rows=[0, 2]),
        )
        # Later declarations win: the fill of row 2's c cell is #EEEEE5.
        .tab_style(style.fill("#EEEEE5"), loc.body(columns="c", rows=[2]))
        .tab_style(text_color("#000005"), loc.body(columns="a"))
        .tab_style(text_color("#000006"), loc.summary(columns="b"))
        .tab_style(text_color("#000007"), loc.grand_summary(columns="d"))
        .tab_style(text_color("#000008"), loc.summary_stub())
        .tab_style(text_color("#000009"), loc.grand_summary_stub())
        .tab_style(style.text(transform="uppercase"), loc.footer())
        .tab_style(style.css("letter-spacing: 1px; padding: 1px 2px"), loc.source_notes())
        .tab_style(text_color("#00000A"), loc.footnotes())
        .tab_footnote("On title.", loc.title())
        .tab_footnote("On sub.", loc.subtitle(), placement="left")
        .tab_footnote("On stubhead.", loc.stubhead())
        .tab_footnote("On d.", loc.column_labels(columns="d"))
        .tab_footnote("On Low.", loc.spanner_labels(ids=["Low"]))
        .tab_footnote("On G1.", loc.row_groups(rows=["G1"]))
        .tab_footnote("On stub r3.", loc.stub(rows=[2]))
        .tab_footnote("On a, auto.", loc.body(columns="a", rows=[0]))
        .tab_footnote("On a, right.", loc.body(columns="a", rows=[1]), placement="right")
        .tab_footnote("On c, left.", loc.body(columns="c", rows=[1]), placement="left")
        .tab_footnote("On d.", loc.body(columns="d", rows=[2]))
        .tab_footnote("On d twice.", loc.body(columns="d", rows=[0]))
        .tab_footnote("On d twice, again.", loc.body(columns="d", rows=[0]))
        .tab_footnote("On summary.", loc.summary(columns="a"))
        .tab_footnote("On grand stub.", loc.grand_summary_stub())
    )


def test_every_location_is_styled_and_marked_as_great_tables_draws_it():
    gt_obj = everywhere()
    ir, ours = emitted(gt_obj)
    theirs = drawn(gt_obj)
    assert ours == theirs
    # Not vacuous: every kind of location has its style and mark.
    assert theirs["title"] == (
        ["Title", "^1"], {"background_color": "#EEEEE1", "color": "#000001", "font_weight": "bold"}
    )
    assert theirs["subtitle"] == (
        ["^2", "Sub"], {"color": "#000001", "font_size": "12px", "font_style": "italic"}
    )
    border = {"border": {"bottom": {"color": "#123456", "style": "solid", "width": "3px"}}}
    assert theirs["head"] == {
        "High": (["High"], border),
        "Stubhead": (["Stubhead", "^3"], {"background_color": "#EEEEE2"}),
        "Low": (["Low", "^4"], {**border, "color": "#000003"}),
        "a": (["a"], {**border, "color": "#000002"}),
        "b": (["b"], border),
        "c": (["c"], border),
        "d": (["d", "^5"], {**border, "color": "#000002"}),
    }
    body = theirs["body"]
    assert body[0] == (["G1", "^6"], {})
    # A number's mark goes before it, anything else's after; two marks are one list.
    assert body[1:6] == [
        (["r1"], {}),
        (["^7", "1"], {"color": "#000005"}),
        (["4.5"], {}),
        (["x"], {"background_color": "#EEEEE4", "font_family": "Courier New, monospace",
                 "text_align": "right"}),
        (["^8,9", "7"], {}),
    ]
    assert body[6:9] == [(["r2"], {"color": "#000004"}), (["2", "^10"], {"color": "#000005"}),
                         (["5.5"], {})]
    assert body[9] == (["^11", "y"], {})
    assert body[11:14] == [(["Max"], {"color": "#000008"}), (["^13", "2.0"], {}),
                           (["5.5"], {"color": "#000006"})]
    assert body[16] == (["G2"], {"background_color": "#EEEEE3"})
    assert body[17] == (["r3", "^12"], {})
    assert body[20][1]["background_color"] == "#EEEEE5"
    assert body[-5:] == [
        (["Sum", "^14"], {"color": "#000009"}), (["6.0"], {}), (["16.5"], {}), (["---"], {}),
        (["24.0"], {"color": "#000007"}),
    ]
    note = {"color": "#00000A", "text_transform": "uppercase"}
    assert theirs["footnotes"][:2] == [(["On title."], note), (["On sub."], note)]
    assert theirs["source_notes"] == [(
        ["Source."],
        {"padding": {"top": "1px", "right": "2px", "bottom": "1px", "left": "2px"},
         "text_transform": "uppercase"},
    )]
    # Every mark is its footnote's.
    marks = {}
    for f in ir["footer"]["footnotes"]:
        marks[f["id"]] = f["mark"]
    refs = [n for n in _nodes(ir) if n["type"] == "footnote_mark"]
    assert len(refs) == 16
    assert all(marks[n["ref"]] == n["mark_text"].rstrip(",") for n in refs)
    assert gridwell.Table(json.dumps(ir)).validate() == []


def _nodes(ir):
    """Every content node of the IR."""
    def walk(o):
        if isinstance(o, dict):
            if "type" in o and ("value" in o or "mark_text" in o):
                yield o
            for v in o.values():
                yield from walk(v)
        elif isinstance(o, list):
            for v in o:
                yield from walk(v)

    return list(walk(ir))


def test_the_blank_head_cells_have_great_tables_look():
    # Blank cells in the spanner rows have the column header's style, and the one
    # just above a label spanning rows in Great Tables' HTML has the label's fill.
    gt_obj = (
        GT(pd.DataFrame({"r": ["x"], "a": [1], "b": [2], "c": [3]}), rowname_col="r")
        .tab_stubhead("S")
        .tab_spanner("Low", ["a"])
        .tab_spanner("High", ["a", "b"])
        .tab_style(style.fill("#AAAAAA"), loc.stubhead())
        .tab_style(style.fill("#BBBBBB"), loc.column_labels(columns="c"))
        .tab_style(style.text(color="#CCCCCC"), loc.column_header())
    )
    ir = gridwell.gt_to_dict(gt_obj)
    defs = ir["styles"]["defs"]
    rows = [[defs.get(c["style_id"], {}) for c in r["cells"]
             if not c["content"] and not c["is_placeholder"]]
            for r in ir["table"]["thead"]["rows"]]
    header = {"color": "#CCCCCC"}
    assert rows == [
        [header, header],  # above the stub, and above c
        [{"background_color": "#AAAAAA"}, {}, {"background_color": "#BBBBBB"}],  # b, then c
        [],
    ]


def test_a_mark_goes_where_the_first_footnote_says():
    df = pd.DataFrame({"n": [1.5], "s": ["x"]})
    for placement, expected in [("left", ["^1", "x"]), ("right", ["x", "^1"]), ("auto", ["x", "^1"])]:
        gt_obj = GT(df).tab_footnote("A.", loc.body(columns="s"), placement=placement)
        assert emitted(gt_obj)[1]["body"][1][0] == expected == drawn(gt_obj)["body"][1][0]
    gt_obj = (
        GT(df)
        .tab_footnote("B.", loc.body(columns="n"), placement="right")
        .tab_footnote("A.", loc.body(columns="n"), placement="left")
    )
    assert emitted(gt_obj)[1]["body"][0][0] == ["1.5", "^1,2"] == drawn(gt_obj)["body"][0][0]


def test_symbol_marks_are_in_the_order_great_tables_gives():
    gt_obj = (
        GT(exibble[["num", "char"]].head(2))
        .opt_footnote_marks("standard")
        .tab_footnote("Two.", loc.body(columns="char", rows=[1]))
        .tab_footnote("One.", loc.column_labels(columns="char"))
        .tab_footnote("Three.", loc.body(columns="char", rows=[1]))
    )
    ir, ours = emitted(gt_obj)
    theirs = drawn(gt_obj)
    assert ours["head"] == theirs["head"]
    assert ours["body"] == theirs["body"]
    assert [t for t, _ in ours["body"]] == [["0.1111"], ["apricot"], ["2.222"], ["banana", "^†,‡"]]


@pytest.mark.parametrize(
    "css,expected",
    [
        ("color: red; color: #00F;", {"color": "#00F"}),
        ("font-size: 12;", {"font_size": "12px"}),
        ("font-size: 1.5em; font-weight: 700 !important;", {"font_size": "1.5em", "font_weight": "700"}),
        ("font-family: 'IBM Plex Sans', \"Helvetica Neue\", sans-serif;",
         {"font_family": "IBM Plex Sans, Helvetica Neue, sans-serif"}),
        ("font-stretch: condensed; letter-spacing: 1px;", {}),
        ("text-indent: 2em; white-space: nowrap", {"indent": "2em", "white_space": "nowrap"}),
        ("padding: 1px 2px 3px", {"padding": {"top": "1px", "right": "2px", "bottom": "3px", "left": "2px"}}),
        ("padding: 4px; padding-left: 0", {"padding": {"top": "4px", "right": "4px", "bottom": "4px", "left": "0"}}),
        ("border-left: 2px dashed rgb(1, 2, 3);",
         {"border": {"left": {"width": "2px", "style": "dashed", "color": "rgb(1, 2, 3)"}}}),
        ("border: thin solid black; border-top: none;",
         {"border": {"top": {"style": "none"},
                     **{s: {"width": "thin", "style": "solid", "color": "black"}
                        for s in ("right", "bottom", "left")}}}),
        ("border-bottom-color: red; border-style: SOLID none",
         {"border": {"bottom": {"color": "red", "style": "solid"}, "top": {"style": "solid"},
                     "right": {"style": "none"}, "left": {"style": "none"}}}),
        ("background-color: url('a;b'); color", {"background_color": "url('a;b')"}),
        ("", {}),
        ("not css at all", {}),
    ],
)
def test_css_is_read_as_style_definitions(css, expected):
    assert _css_style(css) == expected


def test_every_style_great_tables_has_is_valid_ir():
    gt_obj = GT(exibble[["num", "char"]].head(2)).tab_style(
        [
            style.text(color="red", font="Arial", size=12, align="center", v_align="middle",
                       style="italic", weight=300, stretch="condensed", decorate="underline",
                       transform="lowercase", whitespace="pre-wrap"),
            style.fill("#ABCDEF"),
            style.borders(sides="all", color="blue", style="dotted", weight="2px"),
            style.css("max-width: 10px; overflow: hidden; text-overflow: ellipsis"),
        ],
        loc.body(columns="num", rows=[0]),
    )
    ir = gridwell.gt_to_dict(gt_obj)
    cell = ir["table"]["tbody"][0]["rows"][0]["cells"][0]
    edge = {"width": "2px", "style": "dotted", "color": "blue"}
    assert ir["styles"]["defs"][cell["style_id"]] == {
        "color": "red", "font_family": "Arial", "font_size": "12px", "text_align": "center",
        "vertical_align": "middle", "font_style": "italic", "font_weight": "300",
        "text_decoration": "underline", "text_transform": "lowercase", "white_space": "pre-wrap",
        "background_color": "#ABCDEF", "border": {s: edge for s in ("top", "right", "bottom", "left")},
        "max_width": "10px", "overflow": "hidden", "text_overflow": "ellipsis",
    }
    table = gridwell.Table(json.dumps(ir))
    assert table.validate() == []
    assert "#ABCDEF" in table.render_html()


# ─── Table options ───


def defs_of(ir, style_id):
    return ir["styles"]["defs"].get(style_id, {}) if style_id else {}


def test_default_options_add_no_styles():
    ir = gridwell.gt_to_dict(GT(exibble, rowname_col="row", groupname_col="group").grand_summary_rows(
        fns={"Sum": lambda x: x.sum(numeric_only=True)}
    ))
    assert ir["styles"] == {"defs": {}, "compositions": {}, "conditionals": []}


def test_a_theme_is_the_options_styles():
    gt_obj = (
        GT(exibble[["num", "char", "row", "group"]].head(4), rowname_col="row", groupname_col="group")
        .opt_stylize(style=6, color="blue")
        .opt_row_striping()
        .summary_rows(fns={"Max": lambda x: x[["num"]].max()})
        .grand_summary_rows(fns={"Sum": lambda x: x[["num"]].sum()})
        .tab_style(style.text(weight="bold"), loc.stub(rows=[1]))
        .tab_style(style.fill("#FF0000"), loc.summary_stub())
    )
    ir = gridwell.gt_to_dict(gt_obj)
    head = ir["table"]["thead"]["rows"][0]
    assert defs_of(ir, head["style_id"]) == {"background_color": "#0076BA", "color": "#FFFFFF"}
    groups = ir["table"]["tbody"]
    assert defs_of(ir, groups[0]["rows"][0]["style_id"]) == {"border": {"top": {"style": "none"}}}
    assert ir["styles"]["conditionals"] == [{
        "id": "row_striping",
        "selector": {"row_parity": "even", "scope": "tbody"},
        "style": {"background_color": "#EDF7FC", "color": "#333333"},
    }]
    stub = {"background_color": "#89D3FE", "color": "#333333"}
    # The stub isn't striped: its own fill on every row (with tab_style() over it).
    stubs = [defs_of(ir, r["cells"][0]["style_id"]) for g in groups for r in g["rows"]]
    assert stubs == [stub, {**stub, "font_weight": "bold"}, stub, stub]
    # Summary rows: the row's fill; the stub's, the row's, then tab_style()'s in a stub cell.
    summary = groups[0]["summary_rows"][0]
    assert defs_of(ir, summary["style_id"]) == {}
    assert defs_of(ir, summary["cells"][0]["style_id"]) == {"background_color": "#FF0000", "color": "#333333"}
    grand = groups[-1]["summary_rows"][0]
    assert defs_of(ir, grand["style_id"]) == {"background_color": "#D5D5D5", "color": "#333333"}
    table = gridwell.Table(json.dumps(ir))
    assert table.validate() == []
    html = table.render_html()
    assert "#EDF7FC" in html.upper() and "#0076BA" in html.upper()


def test_striping_covers_the_parts_great_tables_stripes():
    # (Groups of odd sizes: the stripes follow the rows shown, not each group's.)
    df = pd.DataFrame({"r": list("abcd"), "g": ["G", "H", "H", "H"], "x": [1, 2, 3, 4]})

    def cells(gt_obj):
        ir = gridwell.gt_to_dict(gt_obj)
        rows = [r for g in ir["table"]["tbody"] for r in g["rows"]]
        return ir, [[defs_of(ir, c["style_id"]) for c in r["cells"]] for r in rows]

    table = {"background_color": "#FFFFFF", "color": "#333333"}
    stripe = {"background_color": "#F4F4F4", "color": "#333333"}
    base = GT(df, rowname_col="r", groupname_col="g")
    # The body only (opt_row_striping()'s): the stub's cells on the stripe rows keep
    # the table's background. Rows are counted across groups.
    ir, got = cells(base.opt_row_striping())
    assert ir["styles"]["conditionals"][0]["style"] == stripe
    assert got == [[{}, {}], [table, {}], [{}, {}], [table, {}]]
    # The stub only.
    _, got = cells(base.tab_options(row_striping_include_stub=True))
    assert got == [[{}, {}], [{}, table], [{}, {}], [{}, table]]
    # The stub with a fill of its own: the stripe over it.
    _, got = cells(
        base.tab_options(
            row_striping_include_stub=True, row_striping_include_table_body=True,
            stub_background_color="#123456", row_striping_background_color="#EEEEEE",
        )
    )
    own = {"background_color": "#123456", "color": "#FFFFFF"}
    assert got == [[own, {}], [{"background_color": "#EEEEEE", "color": "#333333"}, {}]] * 2
    # No striping without either part.
    assert gridwell.gt_to_dict(base.tab_options(row_striping_include_table_body=False))["styles"][
        "conditionals"
    ] == []


def test_options_style_fonts_and_group_labels():
    gt_obj = (
        GT(pd.DataFrame({"g": ["A"], "x": [1]}), groupname_col="g")
        .tab_options(
            column_labels_font_weight="bold",
            row_group_background_color="#000000",
            row_group_font_weight="bold",
            table_font_color="#111111",
            table_font_color_light="#EEEEEE",
        )
        .tab_style(style.text(color="red"), loc.row_groups())
    )
    ir = gridwell.gt_to_dict(gt_obj)
    assert defs_of(ir, ir["table"]["thead"]["rows"][0]["style_id"]) == {"font_weight": "bold"}
    label = ir["table"]["tbody"][0]["label"]
    # The light text color on a dark fill; tab_style() over the options.
    assert defs_of(ir, label["style_id"]) == {
        "background_color": "#000000", "color": "red", "font_weight": "bold"
    }
    assert gridwell.Table(json.dumps(ir)).validate() == []
