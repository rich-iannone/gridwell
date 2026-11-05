"""Conformance: `gridwell.conformance`, and the kit in conformance/ (the expected
grids, the HTML reader that gives their L1, and make.py that writes them)."""

import copy
import dataclasses
import importlib.util
import json
import os
import pickle
import subprocess
import sys
from pathlib import Path

import pytest

import gridwell
from gridwell import InvalidTableError, Table
from gridwell.conformance import (
    MISSING,
    TIERS,
    Difference,
    Report,
    assert_conforms,
    check,
    normalize,
)

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "fixtures"
KIT = ROOT / "conformance"
EXPECTED = KIT / "expected"
GT_REF = ROOT / "harness" / "gt-reference"
PY_REF = ROOT / "harness" / "gt-reference-py"

in_checkout = pytest.mark.skipif(not EXPECTED.is_dir(), reason="not in a source checkout")


def load(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def minimal() -> dict:
    return load(FIXTURES / "minimal" / "minimal_1x1.json")


def cell(ir: dict, row: int = 0, col: int = 0) -> dict:
    return ir["table"]["tbody"][0]["rows"][row]["cells"][col]


# ─── normalize() ───


def test_normalize_takes_any_table():
    ir = minimal()
    grid = normalize(ir)
    assert grid["format"] == "gridwell-conformance-grid/1"
    assert grid["tier"] == "L3"
    assert normalize(json.dumps(ir)) == grid
    assert normalize(Table(ir)) == grid
    assert grid["grid"]["groups"][0]["rows"][0]["cells"][0]["text"] == "Hello"


def test_normalize_to_a_tier():
    ir = minimal()
    l1 = normalize(ir, "L1")
    assert l1["tier"] == "L1"
    assert l1["grid"]["groups"][0]["rows"][0]["cells"] == [{"col": 0, "text": "Hello"}]
    assert "runs" in json.dumps(normalize(ir, "L2"))
    assert '"style"' not in json.dumps(normalize(ir, "L2"))
    assert normalize(ir, "L0")["grid"] == {}
    with pytest.raises(ValueError, match='unknown tier "L4"; tiers are L0, L1, L2 and L3'):
        normalize(ir, "L4")  # type: ignore[arg-type]


def test_normalize_refuses_an_invalid_table():
    with pytest.raises(InvalidTableError, match="COL_COUNT"):
        normalize(load(FIXTURES / "invalid" / "col_count_mismatch.json"))
    with pytest.raises(ValueError):
        normalize("{not json")
    with pytest.raises(TypeError):
        normalize(7)  # type: ignore[arg-type]


# ─── check() ───


def test_check_passes_its_own_grid_in_any_form(tmp_path):
    ir = minimal()
    grid = normalize(ir)
    path = tmp_path / "grid.json"
    path.write_text(json.dumps(grid), encoding="utf-8")
    for expected in (grid, json.dumps(grid), "\n " + json.dumps(grid), path, str(path)):
        report = check(ir, expected)
        assert report.passed, expected
        assert report.tier == "L3"
        assert report.tiers == {"L0": "pass", "L1": "pass", "L2": "pass", "L3": "pass"}
        assert report.errors == ()
        assert report.differences == ()
        assert str(report) == report.text
        assert report.text.startswith("passes L3\n")
    assert check(Table(ir), grid).passed
    assert check(json.dumps(ir), grid).passed


def test_check_reports_each_difference():
    expected = minimal()
    ir = copy.deepcopy(expected)
    cell(ir)["content"] = [{"type": "text", "value": "Bye"}]
    ir["header"]["title"] = {"content": [{"type": "text", "value": "New"}]}
    report = check(ir, normalize(expected))
    assert not report.passed
    assert report.tiers == {"L0": "pass", "L1": "fail", "L2": "fail", "L3": "fail"}
    assert Difference("L1", "groups[0].rows[0].cells[0].text", "Hello", "Bye") in report.differences
    title = next(d for d in report.differences if d.path == "title")
    # Null in the expected grid, not missing from it.
    assert title.expected is None
    assert title.actual == {"runs": [{"text": "New"}], "text": "New"}
    assert str(title) == 'L1 title: expected null, got {"runs":[{"text":"New"}],"text":"New"}'
    assert report.text.startswith("fails L1 (checked to L3)\n")
    # Each difference as the report's text has it.
    assert report.text.endswith("".join(f"  {d}\n" for d in report.differences))


def test_long_values_are_cut_short_as_in_the_report():
    expected = minimal()
    ir = copy.deepcopy(expected)
    cell(ir)["content"] = [{"type": "text", "value": "é" * 200}]
    report = check(ir, normalize(expected))
    assert report.differences[1].actual == "é" * 200
    assert str(report.differences[1]) == f'L1 groups[0].rows[0].cells[0].text: expected "Hello", got "{"é" * 119}…'
    assert report.text.endswith("".join(f"  {d}\n" for d in report.differences))


def test_a_missing_value_is_missing_not_none():
    expected = minimal()
    ir = copy.deepcopy(expected)
    ir["config"]["lang"] = "fr"
    d = check(ir, normalize(expected)).differences
    assert d == (Difference("L2", "lang", MISSING, "fr"),)
    assert str(d[0]) == 'L2 lang: expected nothing, got "fr"'
    d = check(expected, normalize(ir)).differences
    assert d == (Difference("L2", "lang", "fr", MISSING),)
    assert repr(MISSING) == "MISSING"
    assert pickle.loads(pickle.dumps(MISSING)) is MISSING
    assert type(MISSING)() is MISSING


def test_check_to_a_tier():
    expected = minimal()
    ir = copy.deepcopy(expected)
    ir["config"]["lang"] = "fr"
    grid = normalize(expected)
    report = check(ir, grid, "L1")
    assert report.passed
    assert report.tier == "L1"
    assert report.tiers == {"L0": "pass", "L1": "pass", "L2": "not checked", "L3": "not checked"}
    l1 = normalize(expected, "L1")
    assert check(ir, l1).tier == "L1"
    with pytest.raises(ValueError, match="goes to L1, so it can't be checked to L2"):
        check(ir, l1, "L2")
    with pytest.raises(ValueError, match="unknown tier"):
        check(ir, grid, "l9")  # type: ignore[arg-type]


def test_ir_that_isnt_valid_fails_l0():
    grid = normalize(minimal())
    for ir in ("{not json", load(FIXTURES / "invalid" / "col_count_mismatch.json")):
        report = check(ir, grid)
        assert not report.passed
        assert report.tiers["L0"] == "fail"
        assert report.errors
        assert report.differences == ()
    assert check("{not json", grid).errors[0].startswith("the IR doesn't parse: ")


def test_an_expected_grid_that_isnt_one_raises(tmp_path):
    ir = minimal()
    grid = normalize(ir)
    for bad, why in [
        (ir, "has no format"),
        ({**grid, "format": "x"}, 'has format "x"'),
        ({k: v for k, v in grid.items() if k != "tier"}, "has no tier"),
        ("{oops", "isn't JSON"),
    ]:
        with pytest.raises(ValueError, match="the expected grid " + why):
            check(ir, bad)
    with pytest.raises(FileNotFoundError):
        check(ir, tmp_path / "nope.json")
    with pytest.raises(FileNotFoundError):
        check(ir, "nope.json")
    for bad in (7, None, [grid]):
        with pytest.raises(TypeError, match="expected must be a grid"):
            check(ir, bad)  # type: ignore[arg-type]
    for bad in (7, None, b"{}"):
        with pytest.raises(TypeError, match="ir must be IR"):
            check(bad, grid)  # type: ignore[arg-type]


def test_assert_conforms():
    expected = minimal()
    report = assert_conforms(expected, normalize(expected))
    assert isinstance(report, Report) and report.passed
    ir = copy.deepcopy(expected)
    cell(ir)["content"] = [{"type": "text", "value": "Bye"}]
    with pytest.raises(AssertionError) as e:
        assert_conforms(ir, normalize(expected))
    assert str(e.value) == check(ir, normalize(expected)).text
    assert 'L1 groups[0].rows[0].cells[0].text: expected "Hello", got "Bye"' in str(e.value)
    # Not to the tier that differs.
    ir["config"]["lang"] = "fr"
    with pytest.raises(AssertionError):
        assert_conforms(ir, normalize(expected), "L1")
    assert_conforms(ir, normalize(expected), "L0")


def test_reports_are_frozen():
    report = check(minimal(), normalize(minimal()))
    with pytest.raises(dataclasses.FrozenInstanceError):
        report.passed = False  # type: ignore[misc]
    with pytest.raises(dataclasses.FrozenInstanceError):
        Difference("L1", "p", 1, 2).path = "q"  # type: ignore[misc]


def test_the_tiers():
    assert TIERS == ("L0", "L1", "L2", "L3")


def test_the_module_isnt_part_of_the_top_level():
    # Imported on its own; `import gridwell` doesn't need it.
    assert "conformance" not in gridwell.__all__


# ─── The HTML reader ───


def html_grid():
    sys.dont_write_bytecode = True  # no __pycache__ in the source tree
    spec = importlib.util.spec_from_file_location("html_grid", KIT / "html_grid.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def gt_html(thead: str, tbody: str, extra: str = "") -> str:
    return (
        '<div id="x"><style>.gt_table {color: red}</style>'
        f'<table class="gt_table">{extra}<thead>{thead}</thead>'
        f'<tbody class="gt_table_body">{tbody}</tbody></table></div>'
    )


@in_checkout
def test_html_grid_puts_a_label_spanning_rows_at_the_bottom():
    hg = html_grid()
    html = gt_html(
        '<tr class="gt_col_headings gt_spanner_row">'
        '<th class="gt_col_heading" rowspan="2">Name</th>'
        '<th class="gt_center gt_column_spanner_outer" colspan="2">'
        '<span class="gt_column_spanner">Both</span></th></tr>'
        '<tr class="gt_col_headings"><th>  B\n  one </th><th>C</th></tr>',
        '<tr><th class="gt_row gt_left gt_stub">a</th><td class="gt_row">1<br>2</td>'
        '<td class="gt_row">x<span class="gt_footnote_marks">1</span></td></tr>',
    )
    grid = hg.grid(html)["grid"]
    assert grid["head"] == [
        {
            "role": "spanner_label",
            "cells": [{"col": 0, "text": ""}, {"col": 1, "colspan": 2, "text": "Both"}],
        },
        {
            "role": "column_label",
            "cells": [
                {"col": 0, "text": "Name"},
                {"col": 1, "text": "B one"},
                {"col": 2, "text": "C"},
            ],
        },
    ]
    row = grid["groups"][0]["rows"][0]
    assert row == {
        "cells": [
            {"col": 0, "stub": True, "text": "a"},
            {"col": 1, "text": "1\n2"},
            {"col": 2, "text": "x"},
        ]
    }
    assert grid["columns"] == [{"stub": True}, {}, {}]


@in_checkout
def test_html_grid_reads_groups_summaries_and_notes():
    hg = html_grid()
    html = gt_html(
        '<tr class="gt_col_headings"><th>A</th><th>B</th></tr>',
        '<tr class="gt_group_heading_row"><th colspan="2" class="gt_group_heading">G</th></tr>'
        '<tr><td class="gt_row">1</td><td class="gt_row">2</td></tr>'
        '<tr><th class="gt_row gt_stub gt_summary_row">sum</th>'
        '<td class="gt_row gt_summary_row">3</td></tr>'
        '<tr><th class="gt_row gt_stub gt_grand_summary_row">all</th>'
        '<td class="gt_row gt_grand_summary_row">9</td></tr>',
        '<thead><tr class="gt_heading"><td class="gt_heading gt_title">T</td></tr></thead>'
        '<tfoot><tr><td class="gt_footnote"><span class="gt_footnote_marks">1</span> Note.</td></tr>'
        '<tr><td class="gt_sourcenote">Src.</td></tr></tfoot>',
    )
    grid = hg.grid(html)["grid"]
    assert grid["title"] == {"text": "T"}
    assert grid["subtitle"] is None
    assert grid["footnotes"] == [{"text": "Note."}]
    assert grid["source_notes"] == [{"text": "Src."}]
    assert [g["label"] for g in grid["groups"]] == [{"text": "G"}, None]
    assert grid["groups"][0]["summary_rows"] == [
        {"role": "summary_row", "cells": [{"col": 0, "stub": True, "text": "sum"}, {"col": 1, "text": "3"}]}
    ]
    assert grid["groups"][1]["rows"] == []
    assert grid["groups"][1]["summary_rows"][0]["role"] == "grand_summary_row"


@in_checkout
def test_html_grid_places_cells_below_one_spanning_rows():
    hg = html_grid()
    html = gt_html(
        '<tr class="gt_col_headings"><th>A</th><th>B</th></tr>',
        '<tr><th class="gt_row gt_stub" rowspan="2">a</th><td class="gt_row">1</td></tr>'
        '<tr><td class="gt_row">2</td></tr>',
    )
    rows = hg.grid(html)["grid"]["groups"][0]["rows"]
    assert rows == [
        {"cells": [{"col": 0, "rowspan": 2, "stub": True, "text": "a"}, {"col": 1, "text": "1"}]},
        {"cells": [{"col": 1, "text": "2"}]},
    ]


@in_checkout
@pytest.mark.parametrize(
    "markup,shown",
    [
        ("<p>one</p><p>two</p>", "one\ntwo"),
        ("<p>one</p>\n  <p> two </p>\n", "one\ntwo"),
        ("<div>a<div>b</div>c</div>", "a\nb\nc"),
        ("a<p></p>b", "a\nb"),
        ("<p></p><p>a</p><p> </p>", "a"),
        ("x<br><p>y</p>", "x\ny"),
        ("<p>x</p><br>y", "x\n\ny"),
        ("<b>bold</b> <i>it</i>", "bold it"),
        ("a\x00b", "a b"),
    ],
)
def test_html_grid_puts_a_block_on_lines_of_its_own(markup, shown):
    # As a browser does; as gridwell's Great Tables emitter does too.
    assert html_grid().text(html_grid().parse(markup)) == shown


@in_checkout
def test_html_grid_wants_one_gt_table():
    hg = html_grid()
    with pytest.raises(ValueError, match="found 0"):
        hg.grid("<table><tr><td>x</td></tr></table>")
    with pytest.raises(ValueError, match="found 2"):
        hg.grid(gt_html("", "") + gt_html("", ""))


# ─── The kit ───


def cases() -> list[str]:
    return [c["name"] for c in load(GT_REF / "manifest.json")["cases"]]


@in_checkout
def test_the_expected_grids_are_up_to_date():
    result = subprocess.run(
        [sys.executable, str(KIT / "make.py"), "--check"],
        capture_output=True,
        text=True,
        env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
    )
    assert result.returncode == 0, (
        result.stdout + result.stderr + "\nregenerate with: python conformance/make.py"
    )


@in_checkout
def test_make_writes_what_is_out_of_date(tmp_path, monkeypatch, capsys):
    spec = importlib.util.spec_from_file_location("conformance_make", KIT / "make.py")
    make = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(make)
    out = tmp_path / "expected"
    for p in EXPECTED.glob("*/*.json"):
        (out / p.parent.name).mkdir(parents=True, exist_ok=True)
        (out / p.parent.name / p.name).write_bytes(p.read_bytes())
    changed = out / "gt" / "exibble_basic.json"
    changed.write_text("{}", encoding="utf-8")
    (out / "gt" / "gone.json").write_text("{}", encoding="utf-8")
    (out / "great_tables" / "tab_style.json").unlink()
    monkeypatch.setattr(make, "EXPECTED", out)
    monkeypatch.setattr(
        make, "rel", lambda p: p.relative_to(out.parent if out in p.parents else ROOT).as_posix()
    )

    assert make.main(["--check"]) == 1
    assert capsys.readouterr().out.splitlines() == [
        "out of date: expected/great_tables/tab_style.json",
        "out of date: expected/gt/exibble_basic.json",
        "out of date: expected/gt/gone.json",
    ]
    assert changed.read_text(encoding="utf-8") == "{}"
    assert make.main([]) == 0
    assert "22 expected grids, 3 changed" in capsys.readouterr().out
    assert not (out / "gt" / "gone.json").exists()
    assert changed.read_bytes() == (EXPECTED / "gt" / "exibble_basic.json").read_bytes()
    assert make.main(["--check"]) == 0
    assert make.main(["--bogus"]) == 2


@in_checkout
def test_every_case_has_its_expected_grids():
    assert sorted(p.stem for p in (EXPECTED / "gt").glob("*.json")) == sorted(cases())
    assert sorted(p.stem for p in (EXPECTED / "great_tables").glob("*.json")) == sorted(cases())
    for name in cases():
        gt, great_tables = load(EXPECTED / "gt" / f"{name}.json"), load(EXPECTED / "great_tables" / f"{name}.json")
        assert (gt["tier"], great_tables["tier"]) == ("L3", "L3")
        assert gt["case"] == great_tables["case"] == name
        assert gt["producer"]["package"] == "gt"
        assert great_tables["producer"]["package"] == "great_tables"


@in_checkout
@pytest.mark.parametrize("name", cases() if EXPECTED.is_dir() else [])
def test_the_gt_reference_conforms(name):
    assert_conforms((GT_REF / f"{name}.ir.json").read_text(encoding="utf-8"), EXPECTED / "gt" / f"{name}.json")


@in_checkout
@pytest.mark.parametrize("name", cases() if EXPECTED.is_dir() else [])
def test_the_gt_reference_has_what_gts_html_shows(name):
    # make.py refuses to write otherwise; this checks the reader on its own.
    seen = html_grid().grid((GT_REF / f"{name}.gt.html").read_text(encoding="utf-8"))
    assert_conforms((GT_REF / f"{name}.ir.json").read_text(encoding="utf-8"), seen)


@in_checkout
@pytest.mark.parametrize("name", cases() if EXPECTED.is_dir() else [])
def test_the_great_tables_reference_conforms(name):
    assert_conforms(
        (PY_REF / f"{name}.ir.json").read_text(encoding="utf-8"),
        EXPECTED / "great_tables" / f"{name}.json",
    )


@in_checkout
@pytest.mark.parametrize("name", cases() if EXPECTED.is_dir() else [])
def test_the_great_tables_reference_has_what_great_tables_html_shows(name):
    # make.py refuses to write otherwise; this checks the reader on its own.
    seen = html_grid().grid((PY_REF / f"{name}.gt.html").read_text(encoding="utf-8"))
    assert_conforms((PY_REF / f"{name}.ir.json").read_text(encoding="utf-8"), seen)


def great_tables_cases() -> dict:
    """`conformance/cases.py`'s tables (needs Great Tables and pandas)."""
    pytest.importorskip("great_tables")
    pytest.importorskip("pandas")
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location("conformance_cases", KIT / "cases.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.cases


@in_checkout
def test_each_cases_great_tables_table_conforms():
    # gt_to_ir() of the cases, live; only with the Great Tables version the expected
    # grids were made with (another can format values differently).
    built = great_tables_cases()
    import great_tables

    manifest = load(PY_REF / "manifest.json")
    if great_tables.__version__ != manifest["great_tables_version"]:
        pytest.skip(f"expected grids made with Great Tables {manifest['great_tables_version']}")
    assert sorted(built) == sorted(cases())
    emitter = {"great_tables": "upstream", "gridwell": "gridwell"}[manifest["ir_emitter"]]
    for name, case in built.items():
        ir = gridwell.gt_to_ir(case["build"](), emitter=emitter)
        assert_conforms(ir, EXPECTED / "great_tables" / f"{name}.json")


def edge_tables() -> dict:
    """Great Tables tables beyond the kit's cases, for what the emitter does with
    labels, spanners and summary rows."""
    from great_tables import GT, html, loc, md
    import pandas as pd

    df = pd.DataFrame({
        "a": [1, 2, 3], "b": [4.5, 5.5, 6.5], "c": ["x", "y", "z"], "d": [7, 8, 9],
        "g": ["G1", "G1", "G2"], "r": ["r1", "r2", "r3"],
    })
    return {
        "grand summary, no stub": GT(df[["a", "b", "d"]]).grand_summary_rows(
            fns={"Sum": lambda x: x[["a", "d"]].sum(), "**Mean**": lambda x: x.mean(numeric_only=True)}
        ),
        # Summary values are put in Great Tables' HTML as they are.
        "summary values as HTML": GT(df[["a", "b"]]).grand_summary_rows(
            fns={"Sum": lambda x: x.sum()}, fmt=lambda x: [f"<b>{v:.1f}</b>&nbsp;&amp;" for v in x]
        ),
        "summaries on top": GT(df[["a", "b", "g", "r"]], rowname_col="r", groupname_col="g")
        .summary_rows(fns={"Max": lambda x: x[["a", "b"]].max()}, side="top")
        .grand_summary_rows(fns={"Min": lambda x: x[["a"]].min()}, side="top"),
        "group summaries, no stub": GT(df[["a", "b", "g"]], groupname_col="g").summary_rows(
            fns={"Max <1>": lambda x: x[["a", "b"]].max()}
        ),
        "unspanned columns under a higher level": GT(df[["a", "b", "c", "d"]])
        .tab_spanner("Low", ["c", "d"])
        .tab_spanner("High", spanners=["Low"]),
        "spanner levels apart": GT(df[["a", "b", "c", "d"]])
        .tab_spanner("Low", ["a", "b"])
        .tab_spanner("High", ["a", "b", "d"]),
        "hidden column under a spanner": GT(df[["a", "b", "c", "d"]])
        .tab_spanner("S", ["b", "c", "d"])
        .cols_hide("c"),
        "stub under spanners": GT(df[["r", "a", "b", "d"]], rowname_col="r")
        .tab_spanner("S", ["a", "b"])
        .tab_spanner("T", ["a", "b", "d"])
        .tab_stubhead("Rows"),
        "markup in labels": GT(df, rowname_col="r", groupname_col="g")
        .tab_header(title=md("A *b* **c**"), subtitle=html("x<br>y <code>z</code>"))
        .cols_label(a=md("**A**"), b=html("B<sub>2</sub>"), c="c < d & e")
        .tab_spanner(md("*Span*"), ["c", "d"])
        .tab_stubhead(md("**Stub**"))
        .tab_footnote(md("Note *one*"), locations=loc.column_labels(columns="a"))
        .tab_footnote("Plain <two> & more", locations=loc.title())
        .tab_source_note(html("<p>Para one</p><p>Para <b>two</b></p>"))
        .tab_source_note("a < b"),
        "plain strings stay text": GT(df[["a", "b"]])
        .cols_label(a="*not md* <b>", b="x\ny")
        .tab_header(title="Tom & Jerry's <script>")
        .tab_source_note("  spaced   out  "),
        # Body cells are the HTML Great Tables puts in them: `fmt_*()` output, or
        # the data escaped.
        "HTML in body cells": GT(
            pd.DataFrame({
                "r": ["a < b & c", "<b>not bold</b>"],
                "m": ["**md** *it*\n\npara `two`", "- x\n- y"],
                "sci": [0.000123, 12345.6],
                "u": ["{{m^2 s_1}}", "{{kg m^-1}}"],
                "h": ["<svg><text>T</text></svg> &amp; <i>x</i>", "<sup>2q</sup> 10<sup>2<br>3</sup>"],
            }),
            rowname_col="r",
        )
        .fmt_markdown("m")
        .fmt_scientific("sci")
        .fmt_units("u")
        .fmt(lambda x: x, columns="h"),
        "entities and paragraphs": GT(df[["a"]])
        .tab_header(title=html("A&nbsp;B &amp; C&#x2014;D"))
        .tab_source_note(md("line one\n\nline two"))
        .tab_source_note(html("<p>x</p><br>y<br><br><div>z</div><p></p>")),
        **stub_tables(),
    }


def stub_data():
    import pandas as pd

    return pd.DataFrame({
        "g": ["G1", "G1", "G1", "G2", "G2"], "o": ["x", "x", "y", "x", "x"],
        "i": ["p", "q", "r", "s", "t"], "v": [1, 2, 3, 4, 5], "w": [6, 7, 8, 9, 10],
    })


def stub_tables() -> dict:
    """Great Tables tables with a stub of several columns or a row group column."""
    from great_tables import GT, loc, style

    st = stub_data()
    total = {"Total": lambda x: x[["v", "w"]].sum()}

    def multi(**kw):
        return GT(st, rowname_col=["o", "i"], groupname_col="g", **kw)

    def as_column(gt_obj):
        return gt_obj.tab_options(row_group_as_column=True)

    return {
        "stub of two columns, in groups": multi().tab_spanner("S", ["v", "w"]).tab_stubhead("Rows"),
        "stub of two columns, no groups": GT(st.iloc[[0, 1, 3, 4]], rowname_col=["o", "i"]),
        # (Row 3's "x" is beside row 4's, under another "g": not merged.)
        "stub of three columns": GT(st.iloc[[2, 0, 1, 3, 4]], rowname_col=["g", "o", "i"]),
        "spanner levels over a stub of two columns": GT(st, rowname_col=["o", "i"])
        .tab_spanner("S1", ["v", "w"])
        .tab_spanner("S0", ["w"])
        .tab_spanner("Top", ["v", "w"], level=2),
        "a stubhead label for each stub column": multi().tab_stubhead(["A", "B"]),
        "notes on merged stub cells": multi()
        .tab_footnote("outer", locations=loc.stub(rows=[1], columns="o"))
        .tab_footnote("plain", locations=loc.stub(rows=[0])),
        "row group column": as_column(GT(st[["g", "i", "v"]], rowname_col="i", groupname_col="g")),
        "row group column, no stub": as_column(GT(st[["g", "i", "v"]], groupname_col="g")),
        "row group column and a stub of two columns": as_column(multi())
        .tab_spanner("S1", ["v", "w"])
        .tab_spanner("S2", ["v"], level=0),
        "row group column, a stubhead label for each stub column": as_column(multi().tab_stubhead(["A", "B"])),
        "row group column, striped": as_column(multi().opt_row_striping())
        .tab_options(row_striping_include_stub=True),
        "row group column, styled": as_column(multi())
        .tab_style(style.text(weight="bold"), loc.row_groups()),
        "row group column, grand summary": as_column(multi()).grand_summary_rows(fns=total),
    }


@in_checkout
def test_the_great_tables_emitter_has_what_great_tables_html_shows():
    pytest.importorskip("great_tables")
    pytest.importorskip("pandas")
    for name, gt_obj in edge_tables().items():
        seen = html_grid().grid(gt_obj.as_raw_html())
        report = check(gridwell.gt_to_ir(gt_obj, emitter="gridwell"), seen)
        assert report.passed, f"{name}:\n{report}"


@in_checkout
@pytest.mark.parametrize("stub", [["i"], ["o", "i"]])
def test_a_row_group_columns_summaries_differ_from_great_tables_html_as_known(stub):
    # Great Tables' group cell spans the group's summary rows too; the IR's can't
    # (a rowspan stays in its group's rows), so its summary rows start with an
    # empty stub cell under it instead.
    pytest.importorskip("great_tables")
    pytest.importorskip("pandas")
    from great_tables import GT

    gt_obj = (
        GT(stub_data()[["g", *stub, "v", "w"]], rowname_col=stub, groupname_col="g")
        .tab_options(row_group_as_column=True)
        .summary_rows(fns={"Sum": lambda x: x[["v", "w"]].sum()})
    )
    ir = gridwell.gt_to_ir(gt_obj, emitter="gridwell")
    seen = html_grid().grid(gt_obj.as_raw_html())
    assert not check(ir, seen, tier="L1").passed
    for group in seen["grid"]["groups"]:
        assert group["rows"][0]["cells"][0]["rowspan"] == len(group["rows"]) + 1
        group["rows"][0]["cells"][0]["rowspan"] = len(group["rows"])
        for row in group["summary_rows"]:
            row["cells"].insert(0, {"col": 0, "stub": True, "text": ""})
    report = check(ir, seen, tier="L1")
    assert report.passed, str(report)


@in_checkout
def test_a_gt_table_is_checked_as_its_ir():
    great_tables = pytest.importorskip("great_tables")
    pytest.importorskip("pandas")
    tbl = great_tables.GT(great_tables.exibble.head(2))
    grid = normalize(gridwell.gt_to_ir(tbl))
    assert normalize(tbl) == grid
    assert check(tbl, grid).passed
    assert not check(great_tables.GT(great_tables.exibble.head(3)), grid).passed
