"""The Great Tables reference set in harness/gt-reference-py.

The "Great Tables vs gridwell" gallery page renders the checked-in IR there; if
the emitter's output changes, regenerate it with harness/gt-reference-py/make.py.
Only in a source checkout.
"""

import importlib.metadata
import importlib.util
import json
import sys
from pathlib import Path

import pytest

pytest.importorskip("pandas")
pytest.importorskip("great_tables")

import gridwell
from gridwell._gt import has_upstream

REF_DIR = Path(__file__).resolve().parent.parent / "harness" / "gt-reference-py"

if not (REF_DIR / "manifest.json").exists():
    pytest.skip("not in a source checkout", allow_module_level=True)


def _cases() -> dict:
    sys.dont_write_bytecode = True  # no __pycache__ in the source tree
    spec = importlib.util.spec_from_file_location("gt_reference_cases", REF_DIR / "cases.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.cases


MANIFEST = json.loads((REF_DIR / "manifest.json").read_text(encoding="utf-8"))
CASES = _cases()
SAME_GT = importlib.metadata.version("great_tables") == MANIFEST["great_tables_version"]
# The emitter that wrote the reference IR: Great Tables' own or gridwell's copy.
EMITTER = {"great_tables": "upstream", "gridwell": "gridwell"}[MANIFEST["ir_emitter"]]


def test_the_manifest_lists_the_cases():
    assert [c["name"] for c in MANIFEST["cases"]] == list(CASES)
    for c in MANIFEST["cases"]:
        assert c["description"] == CASES[c["name"]]["description"]


# The cases are written for the Great Tables they were made with: another version
# may lack a feature they use, or format values differently.
same_gt = pytest.mark.skipif(
    not SAME_GT, reason=f"references made with Great Tables {MANIFEST['great_tables_version']}"
)


def test_the_references_use_great_tables_own_emitter_once_there_is_one():
    # make.py uses it when there is one; references made before need regenerating
    # (with any Great Tables version: the first with `as_ir()` is a newer one).
    upstream = has_upstream(next(iter(CASES.values()))["build"]())
    assert EMITTER == ("upstream" if upstream else "gridwell"), "regenerate with make.py"


@same_gt
@pytest.mark.parametrize("name", list(CASES))
def test_the_reference_ir_is_up_to_date(name):
    ir = gridwell.gt_to_ir(CASES[name]["build"](), emitter=EMITTER)
    stored = (REF_DIR / f"{name}.ir.json").read_text(encoding="utf-8")
    assert ir + "\n" == stored, f"{name} IR (regenerate with make.py)"


@same_gt
@pytest.mark.parametrize("name", list(CASES))
def test_every_case_is_valid_ir(name):
    table = gridwell.parse_ir(gridwell.gt_to_ir(CASES[name]["build"]()))
    assert table.validate() == []
