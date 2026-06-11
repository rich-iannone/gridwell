import json
from pathlib import Path

import gridwell

FIXTURES = Path(__file__).parent.parent / "fixtures"


def load_fixture(rel_path: str) -> str:
    return (FIXTURES / rel_path).read_text()


# ─── Parse tests ───


def test_parse_from_json():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    assert repr(table) == "Table(ir_version='1.0')"


def test_parse_from_dict():
    json_str = load_fixture("minimal/minimal_1x1.json")
    d = json.loads(json_str)
    table = gridwell.Table.from_dict(d)
    assert repr(table) == "Table(ir_version='1.0')"


def test_parse_ir_function():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.parse_ir(json_str)
    assert repr(table) == "Table(ir_version='1.0')"


def test_parse_invalid_json():
    try:
        gridwell.Table.from_json("{ not valid }")
        assert False, "Should have raised"
    except ValueError as e:
        assert "parse" in str(e).lower() or "JSON" in str(e)


# ─── Validate tests ───


def test_validate_valid():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    errors = table.validate()
    assert errors == []


def test_validate_invalid():
    json_str = load_fixture("invalid/col_count_mismatch.json")
    table = gridwell.Table.from_json(json_str)
    errors = table.validate()
    assert len(errors) > 0


# ─── to_json round-trip ───


def test_to_json():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    output = table.to_json()
    # Should be valid JSON
    parsed = json.loads(output)
    assert parsed["ir_version"] == "1.0"


# ─── Text render tests ───


def test_render_html():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    html = table.render_html()
    assert "<table" in html


def test_render_latex():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    latex = table.render_latex()
    assert "\\begin{" in latex


def test_render_typst():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    typst = table.render_typst()
    assert "#table(" in typst or "table(" in typst


def test_render_rtf():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    rtf = table.render_rtf()
    assert rtf.startswith("{\\rtf1")


def test_render_svg():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    svg = table.render_svg()
    assert "<svg" in svg


def test_render_ansi():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    ansi = table.render_ansi()
    assert len(ansi) > 0


def test_render_pandoc():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    pandoc = table.render_pandoc()
    assert len(pandoc) > 0


def test_render_quarto():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    quarto = table.render_quarto()
    assert len(quarto) > 0


def test_render_by_name():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    html = table.render("html")
    assert "<table" in html


def test_render_unknown_format():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    try:
        table.render("nope")
        assert False, "Should have raised"
    except ValueError as e:
        assert "nope" in str(e)


# ─── Binary render tests ───


def test_render_docx():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    data = table.render_docx()
    assert isinstance(data, bytes)
    assert data[:4] == b"PK\x03\x04"  # ZIP magic


def test_render_xlsx():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    data = table.render_xlsx()
    assert isinstance(data, bytes)
    assert data[:4] == b"PK\x03\x04"


def test_render_pptx():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    data = table.render_pptx()
    assert isinstance(data, bytes)
    assert data[:4] == b"PK\x03\x04"


def test_render_binary_by_name():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    data = table.render_binary("docx")
    assert isinstance(data, bytes)
    assert data[:4] == b"PK\x03\x04"


def test_render_binary_unknown():
    json_str = load_fixture("minimal/minimal_1x1.json")
    table = gridwell.Table.from_json(json_str)
    try:
        table.render_binary("pdf")
        assert False, "Should have raised"
    except ValueError as e:
        assert "pdf" in str(e)
