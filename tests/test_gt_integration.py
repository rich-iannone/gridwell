"""Integration tests: Great Tables GT -> gridwell IR -> all output formats."""

import importlib
import json

import pytest

# Optional dependencies (the `gt` extra): skip this module when they're absent.
pd = pytest.importorskip("pandas")
great_tables = pytest.importorskip("great_tables")
GT, exibble = great_tables.GT, great_tables.exibble

import gridwell


TEXT_FORMATS = ["html", "latex", "typst", "rtf", "svg", "ansi", "pandoc", "quarto"]
BINARY_FORMATS = ["docx", "xlsx", "pptx"]


def make_simple_gt():
    """Simple GT with no grouping or stub."""
    return GT(exibble.head(3))


def make_grouped_gt():
    """GT with row groups, stub column, header, and source note."""
    return (
        GT(exibble, rowname_col="row", groupname_col="group")
        .tab_header(title="Example Table", subtitle="From exibble dataset")
        .tab_source_note(source_note="Source: great_tables package")
    )


def make_styled_gt():
    """GT with column labels and alignment."""
    df = pd.DataFrame({
        "name": ["Alice", "Bob", "Carol"],
        "score": [95.5, 87.2, 91.8],
        "grade": ["A", "B+", "A-"],
    })
    return (
        GT(df)
        .cols_label(name="Student Name", score="Test Score", grade="Letter Grade")
        .cols_align(align="center", columns="grade")
    )


def make_minimal_gt():
    """Minimal 1-column, 1-row GT."""
    df = pd.DataFrame({"x": [42]})
    return GT(df)


GT_EXAMPLES = [
    ("simple", make_simple_gt),
    ("grouped", make_grouped_gt),
    ("styled", make_styled_gt),
    ("minimal", make_minimal_gt),
]


# ─── IR Emission Tests ───


@pytest.mark.parametrize("name,factory", GT_EXAMPLES)
def test_gt_to_ir_produces_valid_json(name, factory):
    gt_obj = factory()
    ir_json = gridwell.gt_to_ir(gt_obj)
    parsed = json.loads(ir_json)
    assert parsed["ir_version"] == "1.0"
    assert parsed["config"]["table_cols"] > 0
    assert parsed["config"]["body_rows"] > 0


@pytest.mark.parametrize("name,factory", GT_EXAMPLES)
def test_gt_to_ir_parses_in_gridwell(name, factory):
    gt_obj = factory()
    ir_json = gridwell.gt_to_ir(gt_obj)
    table = gridwell.Table.from_json(ir_json)
    assert repr(table).startswith("<gridwell.Table: ")


@pytest.mark.parametrize("name,factory", GT_EXAMPLES)
def test_gt_to_ir_validates(name, factory):
    gt_obj = factory()
    ir_json = gridwell.gt_to_ir(gt_obj)
    table = gridwell.Table.from_json(ir_json)
    errors = table.validate()
    assert errors == [], f"Validation errors for {name}: {errors}"


# ─── Text Format Rendering Tests ───


@pytest.mark.parametrize("name,factory", GT_EXAMPLES)
@pytest.mark.parametrize("fmt", TEXT_FORMATS)
def test_gt_renders_to_text_format(name, factory, fmt):
    gt_obj = factory()
    table = gridwell.Table.from_json(gridwell.gt_to_ir(gt_obj))
    result = table.render(fmt)
    assert len(result) > 0, f"{name} -> {fmt} produced empty output"


# ─── Binary Format Rendering Tests ───


@pytest.mark.parametrize("name,factory", GT_EXAMPLES)
@pytest.mark.parametrize("fmt", BINARY_FORMATS)
def test_gt_renders_to_binary_format(name, factory, fmt):
    gt_obj = factory()
    table = gridwell.Table.from_json(gridwell.gt_to_ir(gt_obj))
    data = table.render_binary(fmt)
    assert len(data) > 100, f"{name} -> {fmt} produced too-small output"
    assert data[:4] == b"PK\x03\x04", f"{name} -> {fmt} is not a valid ZIP"


# ─── Specific Content Checks ───


def test_grouped_gt_has_groups_in_ir():
    gt_obj = make_grouped_gt()
    ir = gridwell.gt_to_dict(gt_obj)
    tbody = ir["table"]["tbody"]
    assert len(tbody) == 2  # grp_a and grp_b
    assert tbody[0]["group_id"] == "grp_a"
    assert tbody[1]["group_id"] == "grp_b"


def test_grouped_gt_has_header_in_ir():
    gt_obj = make_grouped_gt()
    ir = gridwell.gt_to_dict(gt_obj)
    assert ir["header"]["title"]["content"][0]["value"] == "Example Table"
    assert ir["header"]["subtitle"]["content"][0]["value"] == "From exibble dataset"


def test_grouped_gt_has_source_note():
    gt_obj = make_grouped_gt()
    ir = gridwell.gt_to_dict(gt_obj)
    assert len(ir["footer"]["source_notes"]) == 1
    assert "great_tables" in ir["footer"]["source_notes"][0]["content"][0]["value"]


def test_grouped_gt_has_stub():
    gt_obj = make_grouped_gt()
    ir = gridwell.gt_to_dict(gt_obj)
    assert ir["config"]["stub_cols"] == 1
    # First cell of first row should be stub
    first_row = ir["table"]["tbody"][0]["rows"][0]
    assert first_row["cells"][0]["is_stub"] is True
    # A row header, as Great Tables writes it (`<th scope="row">`); other cells
    # have no scope.
    for row in ir["table"]["tbody"][0]["rows"]:
        assert row["cells"][0]["scope"] == "row"
        assert all("scope" not in c for c in row["cells"][1:])


def test_styled_gt_preserves_labels():
    gt_obj = make_styled_gt()
    ir = gridwell.gt_to_dict(gt_obj)
    thead_cells = ir["table"]["thead"]["rows"][0]["cells"]
    labels = [c["content"][0]["value"] for c in thead_cells]
    assert "Student Name" in labels
    assert "Test Score" in labels
    assert "Letter Grade" in labels


def test_html_output_contains_table_tag():
    gt_obj = make_grouped_gt()
    table = gridwell.Table.from_json(gridwell.gt_to_ir(gt_obj))
    html = table.render_html()
    assert "<table" in html
    assert "</table>" in html


def test_latex_output_contains_begin():
    gt_obj = make_simple_gt()
    table = gridwell.Table.from_json(gridwell.gt_to_ir(gt_obj))
    latex = table.render_latex()
    assert "\\begin{" in latex


def test_missing_values_show_what_great_tables_shows():
    import re

    from great_tables import GT, exibble

    gt_tbl = GT(exibble.head(6)[["char", "num"]])
    shown = re.findall(r"<td[^>]*>([^<]*)</td>", gt_tbl.as_raw_html())
    ir = gridwell.gt_to_dict(gt_tbl)
    cells = [c for r in ir["table"]["tbody"][0]["rows"] for c in r["cells"]]
    texts = ["".join(n["value"] for n in c["content"]) for c in cells]
    assert texts == shown
    # The missing ones (char row 5, num row 6) have no typed value; the others do.
    assert all(("typed_value" in c) == (i not in (8, 11)) for i, c in enumerate(cells))
    # Cells have no sort key or data type (IR 1.0 has neither).
    assert not any("sort_key" in c or "data_type" in c for c in cells)


def make_typed_gt():
    """GT with a column of each typed kind, some formatted."""
    import datetime as dt

    df = pd.DataFrame({
        "b": [True, False],
        "i": [1, 2],
        "f": [0.5, 1234.5],
        "d": [dt.date(2024, 3, 15), dt.date(2024, 3, 16)],
        "ts": pd.to_datetime(["2024-03-15 14:30:00", "2024-03-15 09:05:30"]),
        "t": [dt.time(14, 30), dt.time(9, 5, 3)],
        "neg": [-1.5, -2.24],
        "cur": [49.95, 1.5],
        "s": ["x", "y"],
        # Booleans and datetimes in object columns, typed by their values.
        "ob": pd.Series([True, False], dtype=object),
        "odt": pd.Series([dt.datetime(2024, 3, 15, 14, 30)] * 2, dtype=object),
    })
    return (
        GT(df)
        .fmt_number(columns="f", decimals=1)
        .fmt_number(columns="neg", decimals=1)
        .fmt_date(columns="d", date_style="month_day_year")
        .fmt_currency(columns="cur")
    )


def test_typed_values_are_json_values_as_displayed():
    ir = gridwell.gt_to_dict(make_typed_gt())
    rows = ir["table"]["tbody"][0]["rows"]
    typed = [[c.get("typed_value") for c in r["cells"]] for r in rows]
    assert typed[0] == [
        {"type": "boolean", "value": True},
        {"type": "integer", "value": 1},
        {"type": "number", "value": 0.5},
        {"type": "date", "value": "2024-03-15"},
        {"type": "datetime", "value": "2024-03-15T14:30:00"},
        {"type": "time", "value": "14:30:00"},
        # The number shown (U+2212 MINUS SIGN read as a minus), not the data's.
        {"type": "number", "value": -1.5},
        # A currency is the number it shows.
        {"type": "number", "value": 49.95},
        {"type": "string", "value": "x"},
        {"type": "boolean", "value": True},
        {"type": "datetime", "value": "2024-03-15T14:30:00"},
    ]
    assert typed[1][:7] == [
        {"type": "boolean", "value": False},
        {"type": "integer", "value": 2},
        # Thousands separators dropped.
        {"type": "number", "value": 1234.5},
        {"type": "date", "value": "2024-03-16"},
        {"type": "datetime", "value": "2024-03-15T09:05:30"},
        {"type": "time", "value": "09:05:03"},
        {"type": "number", "value": -2.2},
    ]


def test_typed_values_are_typed_xlsx_cells():
    import io
    import re
    import zipfile

    table = gridwell.Table.from_json(gridwell.gt_to_ir(make_typed_gt()))
    data = table.render_binary("xlsx")
    sheet = zipfile.ZipFile(io.BytesIO(data)).read("xl/worksheets/sheet1.xml").decode()
    row = re.search(r'<row r="2".*?</row>', sheet).group(0)
    cells = re.findall(r'<c r="([A-Z]+)2"([^>]*)>(.*?)</c>', row)
    # A number (no `t`) or boolean (`t="b"`) cell's value; any other is text.
    got = {}
    for col, attrs, body in cells:
        t = re.search(r' t="(\w+)"', attrs)
        t = t.group(1) if t else "n"
        got[col] = (t, re.search(r"<v>(.*?)</v>", body).group(1)) if t in ("n", "b") else "text"
    assert got == {
        "A": ("b", "1"),  # TRUE
        "B": ("n", "1"),
        "C": ("n", "0.5"),
        "D": ("n", "45366"),  # 2024-03-15
        "E": ("n", "45366.604166666664"),  # 14:30
        "F": ("n", "0.6041666666666666"),
        "G": ("n", "-1.5"),
        "H": ("n", "49.95"),  # $49.95
        "I": "text",  # "x"
        "J": ("b", "1"),
        "K": ("n", "45366.604166666664"),
    }


def make_shown_numbers_gt():
    """GT with numbers displayed in other locales' marks, as currencies,
    percentages, in scientific and compact notation, and with signs and units."""
    df = pd.DataFrame({
        "de": [1234.5, -98765.4321],
        "fr": [1234.5, 0.25],
        "pct": [0.4512, -0.0123],
        "cur": [49.95, -5.0],
        "acc": [49.95, -5.0],
        "eur": [49.95, -5.0],
        "sci": [0.000123, 123456.0],
        "cmp": [1234567.0, 0.5],
        "sgn": [1234.5, -2.0],
        "kg": [1234.5, 2.0],
        "i": [1234, -5],
        "big": [1234.0, 1.234],
    })
    return (
        GT(df)
        .fmt_number("de", decimals=2, locale="de")
        .fmt_number("fr", decimals=2, locale="fr")
        .fmt_percent("pct", decimals=1)
        .fmt_currency("cur")
        .fmt_currency("acc", accounting=True)
        .fmt_currency("eur", currency="EUR", locale="de")
        .fmt_scientific("sci")
        .fmt_number("cmp", compact=True)
        .fmt_number("sgn", decimals=2, force_sign=True)
        .fmt_number("kg", decimals=2, pattern="{x} kg")
        .fmt_integer("i")
        .fmt_number("big", decimals=3)
    )


def test_typed_values_are_the_numbers_any_display_shows():
    ir = gridwell.gt_to_dict(make_shown_numbers_gt())
    rows = ir["table"]["tbody"][0]["rows"]
    shown = [["".join(n["value"] for n in c["content"]) for c in r["cells"]] for r in rows]
    typed = [[c.get("typed_value") for c in r["cells"]] for r in rows]
    assert shown == [
        ["1.234,50", "1\u202f234,50", "45.1%", "$49.95", "$49.95", "€49,95", "1.23 × 10⁻⁴",
         "1.23M", "+1,234.50", "1,234.50 kg", "1,234", "1,234.000"],
        ["−98.765,43", "0,25", "−1.2%", "−$5.00", "($5.00)", "−€5,00", "1.23 × 10⁵",
         "0.50", "−2.00", "2.00 kg", "−5", "1.234"],
    ]
    number = lambda v: {"type": "number", "value": v}  # noqa: E731
    assert typed[0] == [
        # In the locale's marks (once read as 1.2345: the `,` dropped as a separator).
        number(1234.5),
        number(1234.5),
        number(0.451),
        number(49.95),
        number(49.95),
        number(49.95),
        number(0.000123),
        number(1230000.0),
        number(1234.5),
        number(1234.5),
        {"type": "integer", "value": 1234},
        number(1234.0),
    ]
    assert typed[1] == [
        number(-98765.43),
        number(0.25),
        number(-0.012),
        number(-5.0),
        number(-5.0),
        number(-5.0),
        number(123000.0),
        number(0.5),
        number(-2.0),
        number(2.0),
        {"type": "integer", "value": -5},
        # `1.234` is a little more than one here, a thousand and more above.
        number(1.234),
    ]


@pytest.mark.parametrize(
    "shown,raw,value",
    [
        # Rounded displays: the number shown, not the data's.
        ("1,230", 1234.5678, 1230.0),
        ("0", 0.4, 0.0),
        ("-0", -0.2, 0.0),
        # A display only one reading fits is read as shown (`scale_by=`), even
        # when the value doesn't round to it.
        ("1.2", 1234.5678, 1.2),
        ("1.2 MB", 1234567, 1.2),
        ("45.1%", 45.1, 0.451),
        ("1.000", 1, 1.0),
        ("1.000", 1000, 1000.0),
        ("12,34,567", 1234567, 1234567.0),
        ("1.5K", 1.5, 1.5),
        ("1.5K", 1500, 1500.0),
        ("451.20‰", 0.4512, 0.4512),
        ("1.23 × 10^-4", 0.000123, 0.000123),
        ("1.23e-04", 0.000123, 0.000123),
        ("(10%)", -0.1, -0.1),
        ("CA$1,250", 1250, 1250.0),
        ("5,00 kr.", 5, 5.0),
        # Not one number: no typed value.
        ("1,234", 5, None),
        ("<0.001", 0.0001, None),
        ("46 (37, 60)", 46, None),
        ("~5", 5, None),
        ("±5", 5, None),
        ("5/8", 0.625, None),
        ("1.2 m2", 1.2, None),
        ("[5]", 5, None),
        ("(-5)", -5, None),
        ("--5", -5, None),
        ("5-", 5, None),
        ("5%%", 0.05, None),
        ("%5", 0.05, None),
        ("007", 7, None),
        ("1.234.5", 1234.5, None),
        ("1;234", 1234, None),
        ("-(10)", -10, None),
        ("1,234,.5", 1234.5, None),
        (". 5", 5, None),
        ("5 .", 5, None),
        ("1.23M", 1.24, None),
    ],
)
def test_typed_numbers_are_read_off_their_display(shown, raw, value):
    from gridwell._gt_emitter import _typed_value

    typed = _typed_value(raw, shown, "number")
    if value is None:
        assert typed is None
    else:
        assert typed == {"type": "number", "value": value}
        # (`-0` is 0.0, not -0.0.)
        assert str(typed["value"]) == str(value)


def test_typed_integers_are_plain_integers_shown():
    from gridwell._gt_emitter import _typed_value

    assert _typed_value(1234, "1,234", "integer") == {"type": "integer", "value": 1234}
    assert _typed_value(1234, "1.234", "integer") == {"type": "integer", "value": 1234}
    assert _typed_value(-5, "−5", "integer") == {"type": "integer", "value": -5}
    # Shown scaled or with decimals, an integer is a number.
    assert _typed_value(1234567, "1.23M", "integer") == {"type": "number", "value": 1230000.0}
    assert _typed_value(5, "5.00", "integer") == {"type": "number", "value": 5.0}
    assert _typed_value(1, "100%", "integer") == {"type": "number", "value": 1.0}


def _xlsx_number_formats(data, row):
    """Each typed number cell's value and format code in sheet row `row`."""
    import io
    import re
    import zipfile

    z = zipfile.ZipFile(io.BytesIO(data))
    styles = z.read("xl/styles.xml").decode()
    codes = {
        int(i): c.replace("&quot;", '"')
        for i, c in re.findall(r'<numFmt numFmtId="(\d+)" formatCode="([^"]*)"', styles)
    }
    codes.update({0: "General", 1: "0", 2: "0.00", 3: "#,##0", 4: "#,##0.00", 9: "0%", 10: "0.00%"})
    xfs = re.search(r"<cellXfs[^>]*>(.*?)</cellXfs>", styles, re.S).group(1)
    fmt_ids = [int(re.search(r'numFmtId="(\d+)"', x).group(1)) for x in re.findall(r"<xf [^>]*>", xfs)]
    sheet = z.read("xl/worksheets/sheet1.xml").decode()
    cells = re.findall(rf'<c r="([A-Z]+){row}"([^>]*)>(.*?)</c>', sheet)
    out = {}
    for col, attrs, body in cells:
        if " t=" in attrs:
            out[col] = "text"
            continue
        s = re.search(r' s="(\d+)"', attrs)
        fmt = codes[fmt_ids[int(s.group(1))]] if s else "General"
        out[col] = (re.search(r"<v>(.*?)</v>", body).group(1), fmt)
    return out


def test_shown_numbers_are_xlsx_numbers_formatted_as_shown():
    table = gridwell.Table.from_json(gridwell.gt_to_ir(make_shown_numbers_gt()))
    data = table.render_binary("xlsx")
    # Other marks are the reader's (Excel shows a number in its own locale's).
    assert _xlsx_number_formats(data, 2) == {
        "A": ("1234.5", "#,##0.00"),
        "B": ("1234.5", "#,##0.00"),
        "C": ("0.451", "0.0%"),
        "D": ("49.95", '"$"0.00'),
        "E": ("49.95", '"$"0.00'),
        "F": ("49.95", '"€"0.00'),
        # Scientific notation stays as shown.
        "G": "text",
        "H": ("1230000", '0.00,,"M"'),
        "I": ("1234.5", '"+"#,##0.00'),
        "J": ("1234.5", '#,##0.00" kg"'),
        "K": ("1234", "#,##0"),
        "L": ("1234", "#,##0.000"),
    }
    assert _xlsx_number_formats(data, 3) == {
        "A": ("-98765.43", '#,##0.00;"−"#,##0.00'),
        "B": ("0.25", "0.00"),
        "C": ("-0.012", '0.0%;"−"0.0%'),
        "D": ("-5", '"$"0.00;"−$"0.00'),
        "E": ("-5", '"$"0.00;"($"0.00")"'),
        "F": ("-5", '"€"0.00;"−€"0.00'),
        "G": "text",
        "H": ("0.5", "0.00"),
        "I": ("-2", '0.00;"−"0.00'),
        "J": ("2", '0.00" kg"'),
        "K": ("-5", '0;"−"0'),
        "L": ("1.234", "0.000"),
    }


def _header_ids(html):
    """The ids of the `<th>` cells, the `headers` of the `<td>` cells, and the
    table's aria attributes, in document order."""
    from html.parser import HTMLParser

    found = {"th": [], "td": [], "table": []}

    class Parser(HTMLParser):
        def handle_starttag(self, tag, attrs):
            a = dict(attrs)
            if tag == "th":
                found["th"].append(a.get("id"))
            elif tag == "td" and "headers" in a:
                found["td"].append(a["headers"])
            elif tag == "table":
                found["table"].append((a.get("aria-labelledby"), a.get("aria-describedby")))

    Parser().feed(html)
    return found


@pytest.mark.parametrize("table_id", [None, "sales"])
def test_header_ids_are_great_tables_ids(table_id):
    df = pd.DataFrame({
        "name": ["Oslo", "Bergen", "Rome"],
        "grp": ["North East", "North East", "South"],
        "x (km)": [1, 2, 3],
        "y": [4, 5, 6],
    })
    gt_obj = GT(df, rowname_col="name", groupname_col="grp", id=table_id).tab_header(
        title="Sales", subtitle="By region"
    )
    options = {} if table_id is None else {"table_id": table_id}
    ours = gridwell.Table.from_json(gridwell.gt_to_ir(gt_obj)).render_html(**options)
    theirs = _header_ids(gt_obj.as_raw_html())
    assert _header_ids(ours) == theirs
    # Not vacuous: every kind of header is there.
    p = "" if table_id is None else "sales-"
    assert theirs["th"] == [
        None,  # the blank stubhead
        f"{p}x-km",
        f"{p}y",
        f"{p}North-East",
        f"{p}stub-1",
        f"{p}stub-2",
        f"{p}South",
        f"{p}stub-3",
    ]
    assert theirs["td"][0] == f"{p}North-East {p}stub-1 {p}x-km"
    assert theirs["table"] == [(f"{p}title", f"{p}subtitle")]
    # The container has the table's id.
    if table_id is not None:
        assert ours.startswith('<div id="sales"')
    # The id is the IR's, so a render without the option has it too.
    assert gridwell.gt_to_dict(gt_obj)["config"].get("id") == table_id
    plain = gridwell.Table.from_json(gridwell.gt_to_ir(gt_obj)).render_html()
    assert _header_ids(plain) == theirs


@pytest.mark.parametrize(
    "locale,lang,direction",
    [
        ("ar_EG", "ar-EG", "rtl"),
        ("he", "he", "rtl"),
        ("fa", "fa", "rtl"),
        ("pa_Arab", "pa-Arab", "rtl"),
        ("fr_CA", "fr-CA", None),
        ("zh_Hant_TW", "zh-Hant-TW", None),
        ("uz_Latn", "uz-Latn", None),
    ],
)
def test_a_locale_is_the_tables_language_and_direction(locale, lang, direction):
    gt_obj = GT(pd.DataFrame({"x": [1.5]}), locale=locale)
    config = json.loads(gridwell.gt_to_ir(gt_obj))["config"]
    assert config["locale"] == lang
    assert config["lang"] == lang
    assert config.get("dir") == direction
    table = gridwell.Table.from_json(gridwell.gt_to_ir(gt_obj))
    assert table.validate() == []
    html = table.render_html()
    assert f'lang="{lang}"' in html
    assert ('dir="rtl"' in html) == (direction == "rtl")


def test_without_a_locale_the_language_is_unset():
    config = json.loads(gridwell.gt_to_ir(make_simple_gt()))["config"]
    assert config["locale"] == "en-US"
    assert "lang" not in config and "dir" not in config
    html = gridwell.Table.from_json(gridwell.gt_to_ir(make_simple_gt())).render_html()
    assert "lang=" not in html and "dir=" not in html


# ─── Footnotes and source notes ───


def _footnotes_shown(html):
    """(mark, text) of each footnote Great Tables lists under the table."""
    import re

    out = []
    for item in re.findall(r'<td class="gt_footnote"[^>]*>(.*?)</td>', html, re.S):
        mark = re.search(r'<span class="gt_footnote_marks"[^>]*>([^<]*)</span>', item)
        text = re.sub(r"<[^>]+>", "", re.sub(r'<span class="gt_footnote_marks".*?</span>', "", item))
        out.append((mark.group(1) if mark else "", text.strip()))
    return out


def make_footnoted_gt():
    """Footnotes added out of reading order, one text on two cells, one without a mark."""
    from great_tables import loc

    return (
        GT(exibble[["char", "num"]].head(3))
        .tab_header(title="Title")
        .tab_footnote("Body note.", locations=loc.body(columns="char", rows=[1]))
        .tab_footnote("Label note.", locations=loc.column_labels(columns="num"))
        .tab_footnote("Title note.", locations=loc.title())
        .tab_footnote("Body note.", locations=loc.body(columns="num", rows=[2]))
        .tab_footnote("A general note.")
    )


@pytest.mark.skipif(
    not hasattr(
        importlib.import_module("great_tables._utils_render_html"),
        "_process_footnotes_for_display",
    ),
    reason="Great Tables before 1.0 doesn't show footnotes",
)
def test_footnotes_are_the_ones_great_tables_shows_with_its_marks():
    gt_obj = make_footnoted_gt()
    shown = _footnotes_shown(gt_obj.as_raw_html())
    # The probe sees what Great Tables shows: reading order, one per text.
    assert shown == [
        ("", "A general note."),
        ("1", "Title note."),
        ("2", "Label note."),
        ("3", "Body note."),
    ]
    ir = gridwell.gt_to_dict(gt_obj)
    footnotes = ir["footer"]["footnotes"]
    assert [(f["mark"], f["content"][0]["value"]) for f in footnotes] == shown
    assert len({f["id"] for f in footnotes}) == len(footnotes)
    table = gridwell.Table.from_json(gridwell.gt_to_ir(gt_obj))
    assert table.validate() == []
    assert "Title note." in table.render_html()


def test_md_notes_are_their_text_not_the_objects_repr():
    from great_tables import md

    gt_obj = GT(exibble[["num"]].head(1)).tab_source_note(md("Source *two*."))
    content = gridwell.gt_to_dict(gt_obj)["footer"]["source_notes"][0]["content"]
    note = "".join(node["value"] for node in content)
    assert note == "Source two." and "Md(" not in note and "Text(" not in note


def runs(ir, content):
    """Content nodes as (text, style definition) pairs; a line break is ("\\n", None)
    and a footnote mark ("^" + its text, its footnote's id)."""
    defs = ir["styles"]["defs"]
    return [
        ("\n", None) if n["type"] == "line_break"
        else ("^" + n["mark_text"], n["ref"]) if n["type"] == "footnote_mark"
        else (n["value"], defs[n["style_id"]] if n["type"] == "styled_text" else None)
        for n in content
    ]


def test_labels_are_styled_runs_as_great_tables_draws_them():
    from great_tables import html, loc, md

    gt_obj = (
        GT(exibble[["num", "char", "group"]].head(2), groupname_col="group")
        .tab_header(title=md("A *b* **c** ***d***"), subtitle=html("x<br>y <code>z</code>"))
        .cols_label(num=md("**Num**"), char="<b>not bold</b> & *not md*")
        .tab_spanner(html("<i>Span</i>"), ["num", "char"])
        .tab_footnote(md("Note *one*"), locations=loc.title())
        .tab_source_note(html("<p>Para one</p>\n<p>Para <b><i>two</i></b></p><script>x()</script>"))
        .tab_source_note(html("A&nbsp;B &amp; C&#x2014;D <u>u</u> <del>d</del>"))
        # A stray end tag closes nothing; runs of one style are one run.
        .tab_source_note(html("<br> <b>a</i>b</b>c<span>d</span> <br>"))
    )
    ir = gridwell.gt_to_dict(gt_obj)
    bold, italic = {"font_weight": "bold"}, {"font_style": "italic"}
    both = {"font_style": "italic", "font_weight": "bold"}
    assert runs(ir, ir["header"]["title"]["content"]) == [
        ("A ", None), ("b", italic), (" ", None), ("c", bold), (" ", None), ("d", both),
        ("^1", "fn1"),
    ]
    assert runs(ir, ir["header"]["subtitle"]["content"]) == [
        ("x", None), ("\n", None), ("y ", None), ("z", {"font_family": "monospace"})
    ]
    head = ir["table"]["thead"]["rows"]
    assert runs(ir, head[0]["cells"][0]["content"]) == [("Span", italic)]
    assert (head[0]["cells"][0]["colspan"], head[0]["cells"][0]["scope"]) == (2, "colgroup")
    labels = [runs(ir, c["content"]) for c in head[1]["cells"]]
    # A plain string is text as it is, not markup.
    assert labels == [[("Num", bold)], [("<b>not bold</b> & *not md*", None)]]
    assert ir["column_spec"][0]["label"] == "Num"
    footer = ir["footer"]
    assert runs(ir, footer["footnotes"][0]["content"]) == [("Note ", None), ("one", italic)]
    notes = [runs(ir, n["content"]) for n in footer["source_notes"]]
    assert notes == [
        [("Para one", None), ("\n", None), ("Para ", None), ("two", both)],
        [("A\xa0B & C—D ", None), ("u", {"text_decoration": "underline"}), (" ", None),
         ("d", {"text_decoration": "line-through"})],
        [("ab", bold), ("cd", None)],
    ]
    # One definition per distinct style, numbered as the R emitter's.
    ids = [n["style_id"] for part in (ir["header"]["title"], *footer["source_notes"])
           for n in part["content"] if n["type"] == "styled_text"]
    assert ids[:3] == ["gw_1", "gw_2", "gw_3"] and ids[3] == "gw_3"
    gw = {k: v for k, v in ir["styles"]["defs"].items() if k.startswith("gw_")}
    assert len(gw) == len({json.dumps(v, sort_keys=True) for v in gw.values()}) == 6
    table = gridwell.Table(json.dumps(ir))
    assert table.validate() == []
    for fmt in TEXT_FORMATS:
        # (Pandoc's AST, which Quarto's is too, has a node per word.)
        assert ('"one"' if fmt in ("pandoc", "quarto") else "Para one") in table.render(fmt), fmt


def test_summary_rows_get_a_stub_when_the_table_has_none():
    gt_obj = GT(exibble[["num", "currency"]].head(3)).grand_summary_rows(
        fns={"Sum": lambda x: x.sum(numeric_only=True)}
    )
    ir = gridwell.gt_to_dict(gt_obj)
    assert ir["config"]["stub_cols"] == 1
    assert [c["label"] for c in ir["column_spec"]] == ["", "num", "currency"]
    groups = ir["table"]["tbody"]
    data = groups[0]["rows"][0]["cells"]
    assert (data[0]["content"], data[0]["is_stub"], data[0]["scope"]) == ([], True, "row")
    assert groups[-1]["group_id"] is None and groups[-1]["rows"] == []
    (grand,) = groups[-1]["summary_rows"]
    assert grand["role"] == "grand_summary_row"
    stub, *values = grand["cells"]
    assert runs(ir, stub["content"]) == [("Sum", None)]
    assert (stub["is_stub"], stub["scope"]) == (True, "row")
    assert [c["is_stub"] for c in values] == [False, False]
    assert all(c["content"] for c in values)
    assert gridwell.Table(json.dumps(ir)).validate() == []


def test_table_takes_a_gt_directly():
    for make in (make_simple_gt, make_grouped_gt, make_styled_gt):
        gt = make()
        # (Style order in the JSON varies from parse to parse.)
        expected = json.loads(gridwell.parse_ir(gridwell.gt_to_ir(gt)).to_json())
        assert json.loads(gridwell.Table(gt).to_json()) == expected
        assert json.loads(gridwell.Table.from_gt(gt).to_json()) == expected
    r = repr(gridwell.Table(make_grouped_gt()))
    assert 'titled "Example Table"' in r, r
    assert " groups" in r, r


def test_a_gt_displays_in_a_notebook_once_converted():
    html = gridwell.Table(make_grouped_gt())._repr_html_()
    assert html is not None and "Example Table" in html


# ─── Body cells ───


def make_html_body_gt():
    """`fmt_*()` output that is HTML, and HTML of other kinds, in body cells."""
    from great_tables import loc

    df = pd.DataFrame({
        "s": ["a < b & c", "<b>not bold</b>"],
        "m": ["**md** *it*\n\npara `two`", "x"],
        "sci": [0.000123, 12345.6],
        "u": ["{{m^2 s_1}}", "kg"],
        "img": ["a.png", "b.png"],
        "h": [
            # An SVG as written, the text around it kept; one never closed runs on.
            "<svg width='8'><svg/><text>T</text></svg> &amp; <i>x</i><svg/>",
            "<sup>2q</sup> 10<sup>2<br>3</sup> <sub>a1</sub><svg><text>open",
        ],
        "i": [
            # Sizes: a number is pixels, `style` beats the attribute, ones the
            # IR can't take are dropped; `src` is only copied.
            '<img src=" /no/such/file.png " alt="A" width="12" height="9" style="height: 2em !important">',
            '<img src="x.png" width="calc(1px)" style="width: -3px; height: auto">x',
        ],
    })
    return (
        GT(df, rowname_col="s")
        .fmt_markdown("m")
        .fmt_scientific("sci")
        .fmt_units("u")
        # (A URL: Great Tables reads a local path itself to embed it.)
        .fmt_image("img", path="https://example.invalid", height=20)
        .fmt(lambda x: x, columns=["h", "i"])
        .tab_footnote("Note.", locations=loc.body(columns="sci", rows=[0]))
    )


def test_body_cells_are_the_html_great_tables_draws():
    ir = gridwell.gt_to_dict(make_html_body_gt())
    rows = [r["cells"] for r in ir["table"]["tbody"][0]["rows"]]
    bold, italic, mono = {"font_weight": "bold"}, {"font_style": "italic"}, {"font_family": "monospace"}
    # A string is escaped: its text as it is.
    assert [runs(ir, r[0]["content"]) for r in rows] == [
        [("a < b & c", None)], [("<b>not bold</b>", None)]
    ]
    assert runs(ir, rows[0][1]["content"]) == [
        ("md", bold), (" ", None), ("it", italic), ("\n", None), ("para ", None), ("two", mono)
    ]
    # Superscripts in Unicode characters; the mark after the text, as Great Tables
    # places it (the HTML isn't a number).
    assert runs(ir, rows[0][2]["content"]) == [("1.23 × 10⁻⁴", None), ("^1", "fn1")]
    assert runs(ir, rows[1][2]["content"]) == [("1.23 × 10⁴", None)]
    assert runs(ir, rows[0][3]["content"]) == [("m² s₁", None)]
    assert [r[4]["content"] for r in rows] == [
        [{"type": "image", "src": f"https://example.invalid/{f}.png", "alt": "", "height": "20px"}]
        for f in "ab"
    ]
    assert rows[0][5]["content"] == [
        {"type": "raw", "format": "html", "value": "<svg width='8'><svg/><text>T</text></svg>"},
        {"type": "text", "value": " & "},
        {"type": "styled_text", "value": "x", "style_id": rows[0][5]["content"][2]["style_id"]},
        {"type": "raw", "format": "html", "value": "<svg/>"},
    ]
    assert runs(ir, rows[0][5]["content"][1:3]) == [(" & ", None), ("x", italic)]
    # A character without a superscript form, or a line break: as written.
    assert rows[1][5]["content"] == [
        {"type": "text", "value": "2q 102"},
        {"type": "line_break"},
        {"type": "text", "value": "3 ₐ₁"},
        {"type": "raw", "format": "html", "value": "<svg><text>open"},
    ]
    assert [r[6]["content"] for r in rows] == [
        [{"type": "image", "src": "/no/such/file.png", "alt": "A", "width": "12px", "height": "2em"}],
        [{"type": "image", "src": "x.png", "height": "auto"}, {"type": "text", "value": "x"}],
    ]
    # The stub is a row header; nothing else has a scope.
    assert [r[0]["scope"] for r in rows] == ["row", "row"]
    assert not any("scope" in c for r in rows for c in r[1:])
    # The text shown is the typed value, not the markup.
    assert rows[0][1]["typed_value"] == {"type": "string", "value": "md it para two"}
    assert rows[0][3]["typed_value"] == {"type": "string", "value": "m² s₁"}
    # A number in scientific notation is the number (its superscripts read).
    assert rows[0][2]["typed_value"] == {"type": "number", "value": 0.000123}
    assert rows[1][2]["typed_value"] == {"type": "number", "value": 12300.0}
    assert "typed_value" not in rows[0][4]


def test_body_html_is_valid_ir_every_format_draws():
    table = gridwell.Table(make_html_body_gt())
    assert table.validate() == []
    for fmt in TEXT_FORMATS:
        assert table.render(fmt), fmt
    html = table.render("html")
    assert "<svg width='8'><svg/><text>T</text></svg>" in html
    assert "1.23 × 10⁻⁴" in html and "&lt;b&gt;not bold&lt;/b&gt;" in html
    ansi = table.render("ansi")
    assert "m² s₁" in ansi and "a < b & c" in ansi
    for fmt in BINARY_FORMATS:
        assert table.render_binary(fmt)[:4] == b"PK\x03\x04", fmt


def test_superscripts_are_the_characters_gridwells_writers_draw():
    # gridwell lowers raw HTML itself for formats other than HTML; the emitter's
    # superscripts and subscripts are the same characters.
    from gridwell import _gt_emitter

    for tag, chars in _gt_emitter._SCRIPTS.items():
        source = "".join(chars)
        ir = gridwell.gt_to_dict(GT(pd.DataFrame({"x": [1]})))
        ir["table"]["tbody"][0]["rows"][0]["cells"][0]["content"] = [
            {"type": "raw", "format": "html", "value": f"<{tag}>{source}</{tag}>"}
        ]
        drawn = gridwell.Table(json.dumps(ir)).render("ansi")
        assert "".join(chars.values()) in drawn, tag
        assert _gt_emitter._lower_html(f"<{tag}>{source}</{tag}>") == [
            [("".join(chars.values()), {})]
        ]


def stub_data():
    return pd.DataFrame({
        "g": ["G1", "G1", "G1", "G2", "G2"], "o": ["x", "x", "y", "x", "x"],
        "i": ["p", "q", "r", "s", "t"], "v": [1, 2, 3, 4, 5], "w": [6, 7, 8, 9, 10],
    })


def shape(row):
    """A row's cells as (text, rowspan, colspan, scope, is_stub); a placeholder is "·"."""
    out = []
    for c in row["cells"]:
        if c["is_placeholder"]:
            assert c["content"] == [] and c["rowspan"] == c["colspan"] == 1
            out.append("·")
        else:
            text = "".join(n.get("value", "") for n in c["content"])
            out.append((text, c["rowspan"], c["colspan"], c.get("scope"), c["is_stub"]))
    return out


def test_a_stub_of_several_columns_merges_its_outer_cells():
    ir = gridwell.gt_to_dict(
        GT(stub_data(), rowname_col=["o", "i"], groupname_col="g").tab_stubhead("Rows")
    )
    assert ir["config"]["stub_cols"] == 2
    assert [c["align"] for c in ir["column_spec"]] == ["left", "left", "right", "right"]
    (head,) = ir["table"]["thead"]["rows"]
    assert shape(head) == [
        ("Rows", 1, 2, "colgroup", False), "·", ("v", 1, 1, "col", False), ("w", 1, 1, "col", False)
    ]
    # A label over several stub columns is at their left, as Great Tables draws it.
    assert ir["styles"]["defs"][head["cells"][0]["style_id"]] == {"text_align": "left"}
    g1, g2 = ir["table"]["tbody"]
    assert [shape(r) for r in g1["rows"]] == [
        [("x", 2, 1, "rowgroup", True), ("p", 1, 1, "row", True), ("1", 1, 1, None, False),
         ("6", 1, 1, None, False)],
        ["·", ("q", 1, 1, "row", True), ("2", 1, 1, None, False), ("7", 1, 1, None, False)],
        [("y", 1, 1, "row", True), ("r", 1, 1, "row", True), ("3", 1, 1, None, False),
         ("8", 1, 1, None, False)],
    ]
    # A cell isn't merged across groups.
    assert [shape(r)[:2] for r in g2["rows"]] == [
        [("x", 2, 1, "rowgroup", True), ("s", 1, 1, "row", True)], ["·", ("t", 1, 1, "row", True)]
    ]
    assert gridwell.Table(json.dumps(ir)).validate() == []


def test_stub_cells_are_merged_in_the_order_shown():
    # Grouping moves row 2 up beside row 0: the two are merged, row 1 isn't.
    df = pd.DataFrame({"g": ["G1", "G2", "G1"], "o": ["x", "x", "x"], "i": ["p", "q", "r"], "v": [1, 2, 3]})
    ir = gridwell.gt_to_dict(GT(df, rowname_col=["o", "i"], groupname_col="g"))
    g1, g2 = ir["table"]["tbody"]
    assert [shape(r)[:2] for r in g1["rows"]] == [
        [("x", 2, 1, "rowgroup", True), ("p", 1, 1, "row", True)], ["·", ("r", 1, 1, "row", True)]
    ]
    assert [shape(r)[:2] for r in g2["rows"]] == [[("x", 1, 1, "row", True), ("q", 1, 1, "row", True)]]


def test_a_row_group_column_is_the_first_stub_column():
    ir = gridwell.gt_to_dict(
        GT(stub_data(), rowname_col=["o", "i"], groupname_col="g")
        .tab_options(row_group_as_column=True)
        .tab_stubhead("Rows")
        .tab_spanner("S1", ["v", "w"])
        .tab_spanner("S2", ["v", "w"], level=2)
        .summary_rows(fns={"Sum": lambda d: d[["v", "w"]].sum()})
        .grand_summary_rows(fns={"Tot": lambda d: d[["v", "w"]].sum()})
    )
    assert ir["config"]["stub_cols"] == 3
    assert [c["align"] for c in ir["column_spec"]] == ["left"] * 3 + ["right"] * 2
    top, low, labels = ir["table"]["thead"]["rows"]
    # The top spanner row has one blank over the stub; the lowest one per column.
    assert shape(top) == [("", 1, 3, "colgroup", False), "·", "·", ("S2", 1, 2, "colgroup", False), "·"]
    assert shape(low)[:3] == [("", 1, 1, "col", False)] * 3
    assert shape(labels)[:3] == [("Rows", 1, 3, "colgroup", False), "·", "·"]
    g1, g2, grand = ir["table"]["tbody"]
    # The group's label is its first stub cell, spanning its rows; it has no label row.
    assert (g1["label"], g2["label"]) == (None, None)
    assert [shape(r)[:3] for r in g1["rows"]] == [
        [("G1", 3, 1, "rowgroup", True), ("x", 2, 1, "rowgroup", True), ("p", 1, 1, "row", True)],
        ["·", "·", ("q", 1, 1, "row", True)],
        ["·", ("y", 1, 1, "row", True), ("r", 1, 1, "row", True)],
    ]
    assert shape(g2["rows"][0])[0] == ("G2", 2, 1, "rowgroup", True)
    # A group's summary: an empty cell under the group's, the label over the row stub.
    assert [shape(r) for r in g1["summary_rows"]] == [
        [("", 1, 1, None, True), ("Sum", 1, 2, "row", True), "·", ("6", 1, 1, None, False),
         ("21", 1, 1, None, False)]
    ]
    # The grand summary's label spans the whole stub.
    assert shape(grand["summary_rows"][0])[:3] == [("Tot", 1, 3, "row", True), "·", "·"]
    assert gridwell.Table(json.dumps(ir)).validate() == []


def test_a_row_group_column_without_a_stub():
    ir = gridwell.gt_to_dict(
        GT(stub_data()[["g", "i", "v"]], groupname_col="g")
        .tab_options(row_group_as_column=True)
        .grand_summary_rows(fns={"Tot": lambda d: d[["v"]].sum()})
    )
    assert ir["config"]["stub_cols"] == 1
    g1, _, grand = ir["table"]["tbody"]
    assert [shape(r)[0] for r in g1["rows"]] == [("G1", 3, 1, "rowgroup", True), "·", "·"]
    assert shape(grand["summary_rows"][0])[0] == ("Tot", 1, 1, "row", True)
    assert gridwell.Table(json.dumps(ir)).validate() == []


@pytest.mark.parametrize("as_column", [False, True])
def test_a_stubhead_label_for_each_stub_column(as_column):
    ir = gridwell.gt_to_dict(
        GT(stub_data(), rowname_col=["o", "i"], groupname_col="g")
        .tab_options(row_group_as_column=as_column)
        .tab_stubhead(["A", "B"])
    )
    blank = [("", 1, 1, "col", False)] if as_column else []
    (head,) = ir["table"]["thead"]["rows"]
    assert shape(head) == blank + [
        ("A", 1, 1, "col", False), ("B", 1, 1, "col", False), ("v", 1, 1, "col", False),
        ("w", 1, 1, "col", False),
    ]


def test_a_single_stub_column_is_at_the_left():
    df = pd.DataFrame({"r": [1, 2], "v": [3, 4]})
    ir = gridwell.gt_to_dict(GT(df, rowname_col="r"))
    assert [c["align"] for c in ir["column_spec"]] == ["left", "right"]


@pytest.mark.parametrize("fmt", TEXT_FORMATS + BINARY_FORMATS)
def test_a_row_group_column_renders_in_every_format(fmt):
    table = gridwell.Table(gridwell.gt_to_ir(
        GT(stub_data(), rowname_col=["o", "i"], groupname_col="g")
        .tab_options(row_group_as_column=True)
        .summary_rows(fns={"Sum": lambda d: d[["v", "w"]].sum()})
    ))
    if fmt in BINARY_FORMATS:
        assert table.render_binary(fmt)[:4] == b"PK\x03\x04"
    else:
        assert "G1" in table.render(fmt)
