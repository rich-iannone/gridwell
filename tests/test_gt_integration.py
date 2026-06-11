"""Integration tests: Great Tables GT -> gridwell IR -> all output formats."""

import json

import pandas as pd
import pytest
from great_tables import GT, exibble

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
