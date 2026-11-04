"""Delegation: `gt_to_ir()` uses Great Tables' own emitter (`GT.as_ir()`) when the
installed Great Tables has one, and gridwell's copy otherwise.

No released Great Tables has `as_ir()` yet, so these tests give `GT` one.
"""

import json
import warnings

import pytest

pytest.importorskip("pandas")
great_tables = pytest.importorskip("great_tables")
GT, exibble = great_tables.GT, great_tables.exibble

import gridwell
from gridwell._gt import has_upstream


def upstream_ir(version="1.0", title="From Great Tables"):
    """IR unlike anything gridwell's emitter writes for the test table, laid out
    unlike it too (so a converter that re-serializes it is caught)."""
    return json.dumps(
        {
            "ir_version": version,
            "config": {"table_cols": 1, "body_rows": 1, "header_rows": 1},
            "styles": {"defs": {}, "compositions": {}, "conditionals": []},
            "header": {"title": {"content": [{"type": "text", "value": title}]}},
            "column_spec": [{"id": "x"}],
            "table": {
                "thead": {"rows": [{"cells": [{"content": [{"type": "text", "value": "x"}]}]}]},
                "tbody": [{"rows": [{"cells": [{"content": [{"type": "text", "value": "1"}]}]}]}],
            },
        },
        indent=1,
    )


@pytest.fixture
def table():
    return GT(exibble.head(3)).tab_header(title="From gridwell")


@pytest.fixture
def ours(table):
    return gridwell.gt_to_ir(table, emitter="gridwell")


@pytest.fixture
def give_as_ir(monkeypatch):
    """Give `GT` an `as_ir()` that returns `result` (or raises it), recording calls."""
    calls = []

    def give(result):
        def as_ir(self):
            calls.append(self)
            if isinstance(result, BaseException):
                raise result
            return result

        monkeypatch.setattr(GT, "as_ir", as_ir, raising=False)
        return calls

    return give


@pytest.fixture
def no_warnings():
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        yield


# ─── Without an upstream emitter ───


def test_great_tables_has_no_as_ir_yet(table):
    # When one lands, these tests still hold; the reference sets then switch (make.py).
    assert not has_upstream(table)


def test_an_as_ir_that_isnt_a_method_isnt_used(table, ours, monkeypatch, no_warnings):
    monkeypatch.setattr(GT, "as_ir", "1.0", raising=False)
    assert not has_upstream(table)
    assert gridwell.gt_to_ir(table) == ours


def test_auto_uses_gridwells_own_without_a_warning(table, ours, no_warnings):
    assert gridwell.gt_to_ir(table) == ours
    assert gridwell.gt_to_dict(table) == json.loads(ours)


def test_upstream_without_as_ir_raises(table):
    for convert in (gridwell.gt_to_ir, gridwell.gt_to_dict):
        with pytest.raises(RuntimeError, match=r"Great Tables .* has no GT\.as_ir\(\)"):
            convert(table, emitter="upstream")


# ─── With one ───


def test_auto_uses_as_ir_when_there(table, give_as_ir, no_warnings):
    calls = give_as_ir(upstream_ir())
    assert has_upstream(table)
    # Its JSON as it is, and the same parsed.
    assert gridwell.gt_to_ir(table) == upstream_ir()
    assert gridwell.gt_to_dict(table) == json.loads(upstream_ir())
    assert gridwell.gt_to_ir(table, emitter="upstream") == upstream_ir()
    assert calls == [table] * 3


def test_tables_made_from_a_gt_use_as_ir(table, give_as_ir, no_warnings):
    give_as_ir(upstream_ir())
    for made in (gridwell.Table(table), gridwell.Table.from_gt(table)):
        assert made.validate() == []
        assert "From Great Tables" in made.render_html()


def test_gridwell_ignores_as_ir(table, ours, give_as_ir, no_warnings):
    calls = give_as_ir(upstream_ir())
    assert gridwell.gt_to_ir(table, emitter="gridwell") == ours
    assert gridwell.gt_to_dict(table, emitter="gridwell") == json.loads(ours)
    assert calls == []


@pytest.mark.parametrize("version", ["1.0", "1.1", "1.10", "01.0"])
def test_any_ir_1_minor_is_used(table, give_as_ir, no_warnings, version):
    give_as_ir(upstream_ir(version))
    assert json.loads(gridwell.gt_to_ir(table))["ir_version"] == version


# Each bad result, and the reason given for not using it.
BAD = {
    "IR 2": (upstream_ir("2.0"), r"gives IR 2\.0, and this gridwell reads IR 1\.x"),
    "IR 0": (upstream_ir("0.9"), r"gives IR 0\.9, and this gridwell reads IR 1\.x"),
    "IR 10": (upstream_ir("10.0"), r"gives IR 10\.0"),
    "major only": (upstream_ir("1"), r"an ir_version of '1', not MAJOR\.MINOR"),
    "three parts": (upstream_ir("1.0.0"), r"an ir_version of '1\.0\.0'"),
    "padded": (upstream_ir(" 1.0"), r"an ir_version of ' 1\.0'"),
    "trailing newline": (upstream_ir("1.0\n"), r"an ir_version of '1\.0\\n'"),
    "other digits": (upstream_ir("١.٠"), r"an ir_version of '١\.٠'"),
    "number": (upstream_ir(1.0), r"an ir_version of 1\.0, not"),
    "no version": (json.dumps({"config": {}}), r"an ir_version of None"),
    "JSON list": (json.dumps([{"ir_version": "1.0"}]), r"an ir_version of None"),
    "JSON string": (json.dumps("1.0"), r"an ir_version of None"),
    "not JSON": ("<table></table>", r"JSON that doesn't parse"),
    "bytes": (upstream_ir().encode(), r"gave a bytes, not a JSON string"),
    "dict": (json.loads(upstream_ir()), r"gave a dict, not a JSON string"),
    "None": (None, r"gave a NoneType"),
}


@pytest.mark.parametrize("case", list(BAD))
def test_auto_warns_and_uses_its_own_for_a_bad_result(table, ours, give_as_ir, case):
    result, reason = BAD[case]
    give_as_ir(result)
    with pytest.warns(UserWarning, match=reason + r".*; converted with gridwell's own emitter"):
        assert gridwell.gt_to_ir(table) == ours
    with pytest.warns(UserWarning, match=reason):
        assert gridwell.gt_to_dict(table) == json.loads(ours)
    with pytest.warns(UserWarning, match=reason):
        assert "From gridwell" in gridwell.Table(table).render_html()


@pytest.mark.parametrize("case", list(BAD))
def test_upstream_raises_for_a_bad_result(table, give_as_ir, case):
    result, reason = BAD[case]
    give_as_ir(result)
    for convert in (gridwell.gt_to_ir, gridwell.gt_to_dict):
        with pytest.raises(ValueError, match=reason):
            convert(table, emitter="upstream")


def test_an_error_in_as_ir(table, ours, give_as_ir):
    give_as_ir(KeyError("no such column"))
    with pytest.warns(UserWarning, match=r"'no such column'; converted with gridwell's own"):
        assert gridwell.gt_to_ir(table) == ours
    # Its own error, as raised.
    with pytest.raises(KeyError, match="no such column"):
        gridwell.gt_to_ir(table, emitter="upstream")


def test_only_exceptions_are_caught(table, give_as_ir):
    give_as_ir(KeyboardInterrupt())
    with pytest.raises(KeyboardInterrupt):
        gridwell.gt_to_ir(table)


def test_the_warning_points_at_the_caller(table, give_as_ir):
    give_as_ir(upstream_ir("2.0"))
    for convert in (gridwell.gt_to_ir, gridwell.gt_to_dict):
        with pytest.warns(UserWarning) as caught:
            convert(table)
        assert [w.filename for w in caught] == [__file__]


# ─── Arguments ───


def test_an_unknown_emitter_is_a_value_error(table, give_as_ir):
    calls = give_as_ir(upstream_ir())
    for convert in (gridwell.gt_to_ir, gridwell.gt_to_dict):
        for emitter in ("great_tables", "Auto", "", None):
            with pytest.raises(ValueError, match="emitter must be one of 'auto', 'upstream', 'gridwell'"):
                convert(table, emitter=emitter)
    assert calls == []


def test_anything_but_a_gt_is_a_type_error(give_as_ir):
    give_as_ir(upstream_ir())
    for convert in (gridwell.gt_to_ir, gridwell.gt_to_dict):
        for emitter in ("auto", "upstream", "gridwell"):
            for other in (exibble, upstream_ir(), None):
                with pytest.raises(TypeError, match="gt_obj must be a great_tables.GT; got"):
                    convert(other, emitter=emitter)
