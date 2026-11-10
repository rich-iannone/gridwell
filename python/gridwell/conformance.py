"""Conformance: whether an emitter's IR says what an expected table says.

Two correct emitters can write the same table very differently (other style ids,
a composition instead of a definition, `red` or `#FF0000`, keys in another
order), so tables are compared as normalized grids: what a reader sees, with
every style its resolved properties and footnote marks by footnote number.
`normalize()` writes a table's grid; `check()` compares it with an expected one,
in tiers that each include the ones before:

- L0 valid: the IR parses and validates.
- L1 structure: columns and stub, header lines, head rows and spans, groups and
  their labels, row roles, data and summary rows, cell text.
- L2 content: runs, footnote marks, typed values, `scope`,
  column and group ids, `lang` and `dir`.
- L3 styling: resolved styles; alignment; column, table and image sizes.

An emitter's tests can check it against the expected grids in gridwell's
`conformance/` directory:

    from gridwell.conformance import assert_conforms

    assert_conforms(gt_table, "conformance/expected/great_tables/exibble_basic.json")

The CLI has the same: `gridwell conformance normalize` and `gridwell conformance check`.
"""

from __future__ import annotations

import json
import os
import sys
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Any, Final, Literal, TypeAlias, Union

from gridwell._native import Table, _conformance_check, _conformance_normalize

if TYPE_CHECKING:
    from great_tables import GT

__all__ = ["TIERS", "MISSING", "Difference", "Report", "normalize", "check", "assert_conforms"]

Tier = Literal["L0", "L1", "L2", "L3"]

TIERS: Final[tuple[Tier, ...]] = ("L0", "L1", "L2", "L3")

# A table: IR as a JSON string or a dict, a `Table`, or a Great Tables `GT`.
_IR: TypeAlias = Union[str, dict[str, Any], Table, "GT"]
# An expected grid: a dict, a JSON string, or a grid file's path.
_Expected: TypeAlias = Union[str, "os.PathLike[str]", dict[str, Any]]


class _Missing:
    """Not in that grid at all (as opposed to there and `None`)."""

    _instance: _Missing | None = None

    def __new__(cls) -> _Missing:
        if cls._instance is None:
            cls._instance = super().__new__(cls)
        return cls._instance

    def __repr__(self) -> str:
        return "MISSING"

    def __reduce__(self) -> str:
        return "MISSING"


MISSING: Final = _Missing()


@dataclass(frozen=True)
class Difference:
    """One thing that differs between the expected grid and the table's."""

    #: The tier it fails.
    tier: Tier
    #: Where, as `groups[0].rows[2].cells[1].text`.
    path: str
    #: The expected value, or `MISSING` when the expected grid has none.
    expected: Any
    #: The table's value, or `MISSING` when its grid has none.
    actual: Any

    def __str__(self) -> str:
        # As the report's text has it: compact JSON, cut short past 120 characters.
        def show(v: Any) -> str:
            if v is MISSING:
                return "nothing"
            s = json.dumps(v, ensure_ascii=False, separators=(",", ":"))
            return s[:120] + "…" if len(s) > 120 else s

        return f"{self.tier} {self.path}: expected {show(self.expected)}, got {show(self.actual)}"


@dataclass(frozen=True)
class Report:
    """The result of `check()`. `str()` gives it as text."""

    #: Whether the table passes the tier checked.
    passed: bool
    #: The tier checked to.
    tier: Tier
    #: Each tier's status: "pass", "fail" or "not checked" (above the tier checked).
    #: A tier passes when it and every tier below it do.
    tiers: Mapping[Tier, Literal["pass", "fail", "not checked"]]
    #: Why the IR fails L0 (it doesn't parse, or validation errors).
    errors: tuple[str, ...]
    #: Every difference, in grid order; none when the IR fails L0.
    differences: tuple[Difference, ...]
    #: The report as text.
    text: str

    def __str__(self) -> str:
        return self.text


def _table(ir: _IR) -> Table:
    if isinstance(ir, Table):
        return ir
    return Table(ir)


def _is_gt(obj: Any) -> bool:
    gt = sys.modules.get("great_tables")
    return gt is not None and isinstance(obj, getattr(gt, "GT", ()))


def _ir_json(ir: _IR) -> str:
    """IR as JSON text, parsed or not: what doesn't parse fails L0 rather than raising."""
    if isinstance(ir, str):
        return ir
    if isinstance(ir, Table):
        return ir.to_json()
    if isinstance(ir, dict):
        return json.dumps(ir)
    if _is_gt(ir):
        from gridwell import gt_to_ir

        return gt_to_ir(ir)
    raise TypeError(
        "ir must be IR as a JSON string or a dict, a gridwell.Table or a great_tables.GT; "
        f"got {type(ir).__name__}"
    )


def _expected_json(expected: _Expected) -> str:
    if isinstance(expected, dict):
        return json.dumps(expected)
    if isinstance(expected, str) and expected.lstrip().startswith("{"):
        return expected
    if isinstance(expected, (str, os.PathLike)):
        return Path(expected).read_text(encoding="utf-8")
    raise TypeError(
        "expected must be a grid as a dict or a JSON string, or the path of a grid file; "
        f"got {type(expected).__name__}"
    )


def normalize(ir: _IR, tier: Tier = "L3") -> dict[str, Any]:
    """The normalized grid of a table, written to `tier`.

    `ir` is IR as a JSON string or a dict, a `Table`, or a Great Tables `GT`
    (converted with `gridwell.gt_to_ir`). Raises `InvalidTableError` when the IR
    doesn't validate, and `ValueError` for IR that doesn't parse or an unknown tier.
    """
    return json.loads(_conformance_normalize(_table(ir), tier))  # type: ignore[no-any-return]


def check(
    ir: _IR,
    expected: _Expected,
    tier: Tier | None = None,
) -> Report:
    """Check a table against an expected grid, to `tier` (by default, the tier the
    expected grid goes to).

    `ir` is as for `normalize()`; IR that doesn't parse or validate fails L0.
    `expected` is a grid as a dict or a JSON string, or the path of a grid file
    (a string that starts with `{` is JSON). Raises `ValueError` when `expected`
    isn't a grid, or for a tier it doesn't go to.
    """
    report = json.loads(_conformance_check(_ir_json(ir), _expected_json(expected), tier))
    return Report(
        passed=report["passed"],
        tier=report["tier"],
        tiers=report["tiers"],
        errors=tuple(report["errors"]),
        differences=tuple(
            Difference(
                tier=d["tier"],
                path=d["path"],
                expected=d.get("expected", MISSING),
                actual=d.get("actual", MISSING),
            )
            for d in report["differences"]
        ),
        text=report["text"],
    )


def assert_conforms(
    ir: _IR,
    expected: _Expected,
    tier: Tier | None = None,
) -> Report:
    """`check()`, raising `AssertionError` with the report when the table doesn't
    pass; for tests. Returns the report."""
    report = check(ir, expected, tier)
    if not report.passed:
        raise AssertionError(report.text)
    return report
