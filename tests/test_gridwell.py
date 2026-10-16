import json
from pathlib import Path

import pytest

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


# ─── Rendering refuses invalid IR ───

TEXT_RENDERERS = [
    "render_html",
    "render_latex",
    "render_typst",
    "render_rtf",
    "render_svg",
    "render_ansi",
    "render_pandoc",
    "render_quarto",
]
BINARY_RENDERERS = ["render_docx", "render_xlsx", "render_pptx"]


def test_invalid_table_error_is_a_value_error():
    assert issubclass(gridwell.InvalidTableError, ValueError)


@pytest.mark.parametrize("method", TEXT_RENDERERS + BINARY_RENDERERS)
def test_renderers_refuse_invalid_ir(method):
    table = gridwell.Table.from_json(load_fixture("invalid/span_overflow_right.json"))
    with pytest.raises(gridwell.InvalidTableError) as exc:
        getattr(table, method)()
    msg = str(exc.value)
    assert "failed validation" in msg
    assert "[SPAN_OVERFLOW_RIGHT]" in msg


@pytest.mark.parametrize("fmt", ["html", "latex", "typst", "rtf", "svg", "ansi", "pandoc", "quarto"])
def test_render_by_name_refuses_invalid_ir(fmt):
    table = gridwell.Table.from_json(load_fixture("invalid/col_count_mismatch.json"))
    with pytest.raises(gridwell.InvalidTableError, match=r"\[COL_COUNT\]"):
        table.render(fmt)


@pytest.mark.parametrize("fmt", ["docx", "xlsx", "pptx"])
def test_render_binary_by_name_refuses_invalid_ir(fmt):
    table = gridwell.Table.from_json(load_fixture("invalid/col_count_mismatch.json"))
    with pytest.raises(gridwell.InvalidTableError, match=r"\[COL_COUNT\]"):
        table.render_binary(fmt)


def test_unknown_format_still_reported_for_valid_table():
    table = gridwell.Table.from_json(load_fixture("minimal/minimal_1x1.json"))
    with pytest.raises(ValueError, match='unknown format "nope"'):
        table.render("nope")


def test_validate_returns_documented_rule_ids():
    table = gridwell.Table.from_json(load_fixture("invalid/span_overlap.json"))
    errors = table.validate()
    assert errors and all(e.startswith("[") for e in errors)
    assert any(e.startswith("[SPAN_OVERLAP]") for e in errors), errors


def _adjacent_colspans_ir():
    def cell(text, colspan=1):
        return {"content": [{"type": "text", "value": text}], "colspan": colspan}

    placeholder = {"content": [], "is_placeholder": True}
    cells = [cell("A", 2), placeholder, cell("B", 2), placeholder, cell("C"), cell("D")]
    return {
        "ir_version": "1.0",
        "config": {"table_cols": 6, "header_rows": 0, "body_rows": 1},
        "styles": {"defs": {}, "compositions": {}, "conditionals": []},
        "column_spec": [{"id": f"c{i}"} for i in range(6)],
        "table": {"thead": {"rows": []}, "tbody": [{"rows": [{"cells": cells}]}]},
    }


@pytest.mark.parametrize("method", TEXT_RENDERERS + BINARY_RENDERERS)
def test_regression_adjacent_colspans_render(method):
    # Valid IR that used to panic the RTF/SVG writers (surfacing in Python as
    # pyo3_runtime.PanicException) and drop cells in DOCX/PPTX/XLSX.
    table = gridwell.Table.from_dict(_adjacent_colspans_ir())
    assert table.validate() == []
    out = getattr(table, method)()
    assert len(out) > 0
    if isinstance(out, str):
        for label in "ABCD":
            assert label in out, (method, label)


# ─── Format registry and writer options ───


def test_formats_lists_every_format_with_options():
    fmts = gridwell.formats()
    names = [f["name"] for f in fmts]
    assert names == [
        "html", "latex", "typst", "rtf", "svg", "ansi", "pandoc", "quarto",
        "docx", "xlsx", "pptx",
    ]
    by_name = {f["name"]: f for f in fmts}
    assert by_name["html"]["kind"] == "text"
    assert by_name["docx"]["kind"] == "binary"
    assert by_name["html"]["options"]["class_prefix"] == "gw"
    assert by_name["latex"]["options"]["booktabs"] is True
    assert by_name["docx"]["options"] == {}
    assert by_name["quarto"]["extension"] == "json"


def test_options_as_dict_and_as_keywords_agree():
    table = gridwell.Table.from_json(load_fixture("comprehensive/reference_table.json"))
    default = table.render("latex")
    as_dict = table.render("latex", {"booktabs": False})
    as_kwargs = table.render_latex(booktabs=False)
    assert as_dict == as_kwargs != default
    assert "\\toprule" in default and "\\toprule" not in as_dict
    assert table.render("latex", None) == default
    assert table.render_html(inline_styles=True, pretty_print=False).count("\n") == 0


def test_bad_options_raise_invalid_options_error():
    table = gridwell.Table.from_json(load_fixture("minimal/minimal_1x1.json"))
    with pytest.raises(gridwell.InvalidOptionsError, match="unknown field"):
        table.render("html", {"inline_style": True})
    with pytest.raises(gridwell.InvalidOptionsError, match="class_prefix"):
        table.render_html(class_prefix='x"><script>')
    with pytest.raises(gridwell.InvalidOptionsError, match="font_size"):
        table.render_svg(font_size=-1)
    with pytest.raises(gridwell.InvalidOptionsError, match="must be a dict"):
        table.render("html", "inline_styles")
    with pytest.raises(gridwell.InvalidOptionsError, match="unknown field"):
        table.render_binary("docx", {"x": 1})
    # It is still a ValueError.
    assert issubclass(gridwell.InvalidOptionsError, ValueError)


def test_text_and_binary_apis_reject_the_other_kind():
    table = gridwell.Table.from_json(load_fixture("minimal/minimal_1x1.json"))
    with pytest.raises(ValueError, match="binary format"):
        table.render("docx")
    with pytest.raises(ValueError, match="text format"):
        table.render_binary("html")
    assert table.render("HTML").startswith("<div")


# ─── Table.save ───

SAVE_EXTENSIONS = {
    "html": "html",
    "tex": "latex",
    "typ": "typst",
    "rtf": "rtf",
    "svg": "svg",
    "docx": "docx",
    "xlsx": "xlsx",
    "pptx": "pptx",
}


def reference_table():
    return gridwell.Table.from_json(load_fixture("comprehensive/reference_table.json"))


def test_save_infers_the_format_and_writes_the_rendered_bytes(tmp_path):
    table = reference_table()
    kinds = {f["name"]: f["kind"] for f in gridwell.formats()}
    for ext, fmt in SAVE_EXTENSIONS.items():
        path = tmp_path / f"t.{ext}"
        assert table.save(path) is path
        if kinds[fmt] == "text":
            expected = table.render(fmt).encode("utf-8")
        else:
            expected = table.render_binary(fmt)
        assert path.read_bytes() == expected, fmt


def test_save_accepts_str_paths_and_returns_them(tmp_path):
    table = reference_table()
    path = str(tmp_path / "t.RTF")
    assert table.save(path) == path
    assert Path(path).read_bytes() == table.render("rtf").encode("utf-8")


def test_save_explicit_format_overrides_the_extension(tmp_path):
    table = reference_table()
    for fmt in ["pandoc", "quarto"]:
        path = tmp_path / f"{fmt}.json"
        table.save(path, format=fmt)
        assert path.read_text(encoding="utf-8") == table.render(fmt)
    path = tmp_path / "no-extension"
    table.save(path, format="latex")
    assert path.read_text(encoding="utf-8") == table.render("latex")


def test_save_txt_is_plain_unless_color(tmp_path):
    table = reference_table()
    path = tmp_path / "t.txt"
    table.save(path)
    plain = path.read_text(encoding="utf-8")
    assert "\x1b" not in plain
    assert plain == table.render_ansi(color=False)
    table.save(path, color=True)
    assert path.read_text(encoding="utf-8") == table.render_ansi()
    assert "\x1b" in table.render_ansi()


def test_save_passes_options_through(tmp_path):
    table = reference_table()
    path = tmp_path / "t.tex"
    table.save(path, booktabs=False)
    assert path.read_text(encoding="utf-8") == table.render_latex(booktabs=False)


def test_save_errors(tmp_path):
    table = reference_table()
    with pytest.raises(ValueError, match="pandoc or quarto.*format="):
        table.save(tmp_path / "t.json")
    with pytest.raises(ValueError, match=r'no format writes "\.pdf" files'):
        table.save(tmp_path / "t.pdf")
    with pytest.raises(ValueError, match="no file extension"):
        table.save(tmp_path / "t")
    with pytest.raises(ValueError, match="unknown format"):
        table.save(tmp_path / "t.html", format="nope")
    with pytest.raises(gridwell.InvalidOptionsError, match="unknown field"):
        table.save(tmp_path / "t.html", nope=1)
    with pytest.raises(OSError, match="missing-dir"):
        table.save(tmp_path / "missing-dir" / "t.html")
    invalid = gridwell.Table.from_json(load_fixture("invalid/col_count_mismatch.json"))
    with pytest.raises(gridwell.InvalidTableError):
        invalid.save(tmp_path / "invalid.docx")
    with pytest.raises(TypeError):
        table.save(42)
    assert list(tmp_path.iterdir()) == []


def test_save_html_is_a_fragment_unless_standalone(tmp_path):
    table = reference_table()
    path = tmp_path / "t.html"
    table.save(path)
    fragment = path.read_text(encoding="utf-8")
    assert fragment == table.render_html()
    assert not fragment.startswith("<!doctype")
    table.save(path, standalone=True)
    doc = path.read_text(encoding="utf-8")
    assert doc.startswith("<!doctype html>")
    assert '<meta charset="utf-8">' in doc
    assert fragment in doc
    assert doc == table.render_html(standalone=True)
