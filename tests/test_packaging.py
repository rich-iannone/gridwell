"""Packaging: the core package must work without the optional `gt` dependencies."""

import importlib.metadata
import subprocess
import sys
import textwrap
from pathlib import Path

import pytest

import gridwell

FIXTURES = Path(__file__).parent.parent / "fixtures"

# Runs in a fresh interpreter with pandas / great_tables / polars made unimportable,
# simulating an environment where only `pip install gridwell` was run.
WITHOUT_OPTIONAL_DEPS = textwrap.dedent(
    """
    import sys, importlib.abc

    BLOCKED = ("pandas", "great_tables", "polars")

    class Blocker(importlib.abc.MetaPathFinder):
        def find_spec(self, name, path=None, target=None):
            if name.split(".")[0] in BLOCKED:
                raise ModuleNotFoundError(f"No module named {name!r} (blocked by test)")
            return None

    sys.meta_path.insert(0, Blocker())
    for mod in list(sys.modules):
        if mod.split(".")[0] in BLOCKED:
            del sys.modules[mod]

    import gridwell

    # Core API works.
    json_str = open(sys.argv[1]).read()
    table = gridwell.Table.from_json(json_str)
    assert table.validate() == []
    assert "<table" in table.render_html()
    assert table.render_docx()[:2] == b"PK"

    # Nothing optional was imported along the way.
    leaked = [m for m in sys.modules if m.split(".")[0] in BLOCKED]
    assert not leaked, leaked

    # The emitter fails with an actionable message, both ways of reaching it.
    for attempt in ("gridwell.gt_to_ir", "from gridwell import gt_to_dict"):
        try:
            if attempt.startswith("from"):
                exec(attempt)
            else:
                gridwell.gt_to_ir
        except ImportError as e:
            assert "pip install 'gridwell[gt]'" in str(e), str(e)
            assert isinstance(e.__cause__, ImportError)
        else:
            raise AssertionError(f"{attempt} should have raised ImportError")

    print("ok")
    """
)


def test_import_and_render_without_optional_dependencies():
    result = subprocess.run(
        [sys.executable, "-c", WITHOUT_OPTIONAL_DEPS, str(FIXTURES / "minimal/minimal_1x1.json")],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stderr
    assert result.stdout.strip() == "ok"


def test_unknown_attribute_is_attribute_error():
    with pytest.raises(AttributeError, match="no attribute 'nope'"):
        gridwell.nope  # noqa: B018


def test_dir_lists_lazy_names():
    names = dir(gridwell)
    for name in gridwell.__all__:
        assert name in names, name


def test_all_names_resolve_when_gt_extra_installed():
    pytest.importorskip("pandas")
    pytest.importorskip("great_tables")
    for name in gridwell.__all__:
        assert callable(getattr(gridwell, name)), name
    from gridwell import gt_to_dict, gt_to_ir  # noqa: F401


def test_version_matches_distribution_metadata():
    assert gridwell.__version__ == importlib.metadata.version("gridwell")


def test_distribution_metadata():
    meta = importlib.metadata.metadata("gridwell")
    assert meta["Requires-Python"] == ">=3.10"
    # No hard runtime dependencies; Great Tables support is the `gt` extra.
    requires = importlib.metadata.requires("gridwell") or []
    assert all("extra ==" in r for r in requires), requires
    assert "gt" in (meta.get_all("Provides-Extra") or [])
