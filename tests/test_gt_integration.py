"""Integration tests: Great Tables GT -> gridwell IR -> all output formats."""

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
    assert repr(table).startswith("Table(")


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
    # The missing ones (char row 5, num row 6) sort as missing and have no typed
    # value.
    missing = [cells[8], cells[11]]
    for c in missing:
        assert c["sort_key"] is None
        assert "typed_value" not in c
    assert all(c["sort_key"] is not None for i, c in enumerate(cells) if i not in (8, 11))


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
        # A display that isn't a plain number has no typed value, as in R.
        None,
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
        "H": "text",  # $49.95
        "I": "text",  # "x"
        "J": ("b", "1"),
        "K": ("n", "45366.604166666664"),
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
