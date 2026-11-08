"""Type stubs: `gridwell/_native.pyi` must describe the native module as it is.

The checks read the installed stub (what ships in the wheel) and compare it with the
module: names, signatures and docstrings; each per-format method's options and
defaults with `formats()`; then mypy's stubtest, and mypy on code that uses the
stubs well and badly. The mypy checks skip without mypy, unless
GRIDWELL_REQUIRE_MYPY is set (as in CI).
"""

import ast
import importlib.resources
import importlib.util
import inspect
import os
import re
import subprocess
import sys
import textwrap
from pathlib import Path

import pytest

import gridwell
import gridwell._native as native

FIXTURES = Path(__file__).parent.parent / "fixtures"
PACKAGE = importlib.resources.files("gridwell")
STUB = ast.parse((PACKAGE / "_native.pyi").read_text())
FORMATS = {f["name"]: f for f in gridwell.formats()}
HAS_GT = importlib.util.find_spec("great_tables") is not None

# The format a `render_<name>` shorthand renders.
SHORTHANDS = {f"render_{name}": name for name in FORMATS}


def stub_defs(body):
    return {n.name: n for n in body if isinstance(n, (ast.FunctionDef, ast.ClassDef))}


def type_check_only(node):
    return any(isinstance(d, ast.Name) and d.id == "type_check_only" for d in node.decorator_list)


STUB_TOP = stub_defs(STUB.body)
STUB_TABLE = stub_defs(STUB_TOP["Table"].body)


def stub_params(fn):
    """A stub function's parameters (without self / cls) as (name, kind, default),
    with `...` as the default when the stub doesn't give a value."""
    a = fn.args
    positional = a.posonlyargs + a.args
    defaults = [inspect.Parameter.empty] * (len(positional) - len(a.defaults)) + [
        ast.literal_eval(d) for d in a.defaults
    ]
    params = [
        (p.arg, inspect.Parameter.POSITIONAL_OR_KEYWORD, d) for p, d in zip(positional, defaults)
    ]
    if a.vararg:
        params.append((a.vararg.arg, inspect.Parameter.VAR_POSITIONAL, inspect.Parameter.empty))
    for p, d in zip(a.kwonlyargs, a.kw_defaults):
        default = inspect.Parameter.empty if d is None else ast.literal_eval(d)
        params.append((p.arg, inspect.Parameter.KEYWORD_ONLY, default))
    if a.kwarg:
        params.append((a.kwarg.arg, inspect.Parameter.VAR_KEYWORD, inspect.Parameter.empty))
    is_method = positional and positional[0].arg in ("self", "cls")
    return params[1:] if is_method else params


def runtime_params(obj):
    """A native function's parameters (without self), from its text signature."""
    params = [(p.name, p.kind, p.default) for p in inspect.signature(obj).parameters.values()]
    return [p for p in params if p[0] != "self"]


def runtime_table_members():
    return {
        name
        for name, value in vars(native.Table).items()
        if name in ("__new__", "__repr__", "__str__")
        or (not name.startswith("__") and (callable(value) or isinstance(value, staticmethod)))
    }


# ─── What ships ───


def test_the_stub_and_marker_ship_next_to_the_native_module():
    assert (PACKAGE / "py.typed").is_file()
    assert (PACKAGE / "_native.pyi").is_file()
    assert Path(native.__file__).parent == Path(str(PACKAGE / "_native.pyi")).parent


# ─── Names and signatures ───


def test_the_stub_has_the_modules_names():
    public = {
        name
        for name, node in STUB_TOP.items()
        if not name.startswith("_") and not type_check_only(node)
    }
    assert public == {n for n in native.__all__ if not n.startswith("_")}
    runtime = {n for n in dir(native) if not n.startswith("_")}
    assert public == runtime
    # The private functions gridwell.conformance calls.
    private = {name for name in STUB_TOP if name.startswith("_conformance_")}
    assert private == {n for n in native.__all__ if n.startswith("_")}
    declared = next(
        n.value for n in STUB.body if isinstance(n, ast.Assign) and n.targets[0].id == "__all__"
    )
    assert ast.literal_eval(declared) == native.__all__
    assert {n for n in gridwell.__all__ if not n.startswith("gt_")} == public


def test_the_stub_has_the_tables_methods():
    assert set(STUB_TABLE) == runtime_table_members()
    static = {
        name
        for name, node in STUB_TABLE.items()
        if any(isinstance(d, ast.Name) and d.id == "staticmethod" for d in node.decorator_list)
    }
    assert static == {n for n, v in vars(native.Table).items() if isinstance(v, staticmethod)}


def test_the_exceptions_are_value_errors_in_both():
    for name in ("InvalidTableError", "InvalidOptionsError"):
        bases = [ast.unparse(b) for b in STUB_TOP[name].bases]
        assert bases == ["ValueError"], name
        assert getattr(native, name).__bases__ == (ValueError,), name


@pytest.mark.parametrize(
    "path",
    ["parse_ir", "formats", "_conformance_normalize", "_conformance_check"]
    + ["Table", "Table.from_gt", "Table.from_json", "Table.from_dict"]
    + [
        f"Table.{name}"
        for name in ("validate", "warnings", "to_json", "render", "render_binary", "save")
    ]
    + ["Table._repr_html_"],
)
def test_signatures_match_the_module(path):
    obj = native
    for part in path.split("."):
        obj = getattr(obj, part)
    if path == "Table":
        stub = STUB_TABLE["__new__"]
    elif "." in path:
        stub = STUB_TABLE[path.split(".")[1]]
    else:
        stub = STUB_TOP[path]
    assert stub_params(stub) == runtime_params(obj)


@pytest.mark.parametrize("method", sorted(SHORTHANDS))
def test_each_shorthand_takes_its_formats_options_and_defaults(method):
    info = FORMATS[SHORTHANDS[method]]
    assert runtime_params(getattr(native.Table, method)) == [
        ("options", inspect.Parameter.VAR_KEYWORD, inspect.Parameter.empty)
    ]
    stub = STUB_TABLE[method]
    params = stub_params(stub)
    # Keyword-only, one per option; no **options, so a misspelled one is an error.
    assert all(kind is inspect.Parameter.KEYWORD_ONLY for _, kind, _ in params), params
    assert {name for name, _, _ in params} == set(info["options"])
    annotations = {p.arg: ast.unparse(p.annotation) for p in stub.args.kwonlyargs}
    for name, _, default in params:
        expected = info["options"][name]
        if default is Ellipsis:
            # Only a default that isn't a plain value may be left out.
            assert isinstance(expected, (list, dict)), name
            continue
        assert default == expected and type(default) is type(expected), name
        annotation = {
            bool: "bool",
            int: "int",
            float: "float",
            str: "str",
            type(None): "str | None",
        }[type(expected)]
        assert annotations[name] == annotation, name
    returns = {"text": "str", "binary": "bytes"}[info["kind"]]
    assert ast.unparse(stub.returns) == returns


def test_every_format_has_a_shorthand_and_nothing_else_does():
    rendering = {n for n in STUB_TABLE if n.startswith("render_")}
    assert rendering - {"render_binary"} == set(SHORTHANDS)


@pytest.mark.parametrize("method", sorted(SHORTHANDS))
def test_the_stubs_defaults_render_as_no_options_do(method):
    table = gridwell.Table.from_json((FIXTURES / "minimal/minimal_1x1.json").read_text())
    given = {name: d for name, _, d in stub_params(STUB_TABLE[method]) if d is not Ellipsis}
    fmt = SHORTHANDS[method]
    by_name = table.render if FORMATS[fmt]["kind"] == "text" else table.render_binary
    out = getattr(table, method)(**given)
    assert out == getattr(table, method)()
    assert out == by_name(fmt, given)


def test_format_info_describes_formats():
    info = STUB_TOP["FormatInfo"]
    assert type_check_only(info)
    fields = {
        n.target.id: ast.unparse(n.annotation) for n in info.body if isinstance(n, ast.AnnAssign)
    }
    for entry in FORMATS.values():
        assert set(entry) == set(fields)
        assert entry["kind"] in ("text", "binary")
        for name in ("name", "description", "extension", "media_type"):
            assert isinstance(entry[name], str), name
        assert isinstance(entry["experimental"], bool)
        assert isinstance(entry["options"], dict)
    assert fields["experimental"] == "bool"
    assert fields["kind"] == "Literal['text', 'binary']"


# ─── Docstrings ───


def docstring_targets():
    yield "module", native, STUB
    for name in ("parse_ir", "formats", "InvalidTableError", "InvalidOptionsError", "Table"):
        yield name, getattr(native, name), STUB_TOP[name]
    # Private, for gridwell.conformance; their stubs still say what they are.
    for name in ("_conformance_normalize", "_conformance_check"):
        yield name, getattr(native, name), STUB_TOP[name]
    for name in sorted(runtime_table_members()):
        yield f"Table.{name}", getattr(native.Table, name), STUB_TABLE.get(name)


@pytest.mark.parametrize("label,obj,stub", [pytest.param(*t, id=t[0]) for t in docstring_targets()])
def test_docstrings_match_the_module(label, obj, stub):
    assert stub is not None, f"{label} isn't in the stub"
    name = label.rsplit(".", 1)[-1]
    inherited = getattr(getattr(object, name, None), "__doc__", None)
    runtime = obj.__doc__
    if runtime is None or runtime == inherited:
        # Nothing of ours to copy (`__new__`, `__repr__`, `__str__`).
        assert ast.get_docstring(stub) is None, label
    else:
        assert ast.get_docstring(stub) == inspect.cleandoc(runtime), label


@pytest.mark.parametrize("method", sorted(SHORTHANDS))
def test_each_shorthands_docstring_lists_its_options(method):
    doc = " ".join(getattr(native.Table, method).__doc__.split())
    options = set(FORMATS[SHORTHANDS[method]]["options"])
    listed = re.search(r"Options: ([a-z_, ]+)\.", doc)
    if options:
        assert listed, doc
        assert set(listed.group(1).split(", ")) == options
    else:
        assert "Options:" not in doc


# ─── mypy ───


def need_mypy():
    if importlib.util.find_spec("mypy") is None:
        if os.environ.get("GRIDWELL_REQUIRE_MYPY"):
            pytest.fail("GRIDWELL_REQUIRE_MYPY is set but mypy isn't installed")
        pytest.skip("mypy isn't installed")


def mypy_config(tmp_path):
    # pandas has no inline types; without pandas-stubs mypy can't read it.
    config = tmp_path / "mypy.ini"
    config.write_text("[mypy]\nignore_missing_imports = True\n")
    return config


def test_stubtest_finds_nothing(tmp_path):
    need_mypy()
    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "mypy.stubtest",
            # The stub; the package itself can't import its emitter without the extra.
            "gridwell._native",
            "--mypy-config-file",
            str(mypy_config(tmp_path)),
        ],
        capture_output=True,
        text=True,
        cwd=tmp_path,
    )
    assert result.returncode == 0, result.stdout + result.stderr


def run_mypy(tmp_path, code):
    source = tmp_path / "usage.py"
    source.write_text(textwrap.dedent(code))
    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "mypy",
            "--strict",
            "--no-error-summary",
            "--cache-dir",
            str(tmp_path / ".mypy_cache"),
            "--config-file",
            str(mypy_config(tmp_path)),
            str(source),
        ],
        capture_output=True,
        text=True,
        cwd=tmp_path,
    )
    # 0: no errors, 1: errors found; anything else means mypy itself failed.
    assert result.returncode in (0, 1), result.stdout + result.stderr
    errors = [line for line in result.stdout.splitlines() if ": error:" in line]
    lines = {int(line.split(":")[1]) for line in errors}
    marked = {
        i
        for i, line in enumerate(source.read_text().splitlines(), start=1)
        if line.rstrip().endswith("# E")
    }
    return lines, marked, result.stdout + result.stderr


GOOD = """
    from pathlib import Path
    from typing import Any, Literal

    from typing_extensions import assert_type

    import gridwell
    from gridwell import InvalidOptionsError, InvalidTableError, Table

    t = Table("{}")
    assert_type(t, Table)
    assert_type(Table({"ir_version": "1.0"}), Table)
    assert_type(Table(t), Table)
    assert_type(Table.from_json("{}"), Table)
    assert_type(Table.from_dict({}), Table)
    assert_type(gridwell.parse_ir("{}"), Table)
    assert_type(t.validate(), list[str])
    assert_type(t.warnings(), list[str])
    assert_type(t.warnings("html"), list[str])
    assert_type(t.to_json(), str)
    assert_type(t.render("html"), str)
    assert_type(t.render("html", {"inline_styles": True}), str)
    assert_type(t.render_binary("docx"), bytes)
    assert_type(t.save(Path("t.html"), inline_styles=True), Path)
    assert_type(t.save("t.json", "pandoc"), str)
    assert_type(t.render_html(inline_styles=True, table_id="x", class_prefix="p"), str)
    assert_type(t.render_html(table_id=None, quarto_disable_processing=True), str)
    assert_type(t.render_latex(longtable=True, longtable_threshold=10, booktabs=False), str)
    assert_type(t.render_typst(repeat_header=False), str)
    assert_type(t.render_rtf(), str)
    assert_type(t.render_svg(font_size=12, row_height=20.5, font_family="serif"), str)
    assert_type(t.render_ansi(max_width=80, color=False, box_drawing=False), str)
    assert_type(t.render_pandoc(), str)
    assert_type(t.render_quarto(table_id="tbl-x", extra_attrs=[("a", "b")]), str)
    assert_type(t.render_docx(), bytes)
    assert_type(t.render_xlsx(), bytes)
    assert_type(t.render_pptx(base_styles=False), bytes)
    assert_type(t._repr_html_(), str | None)
    assert_type(repr(t), str)
    info = gridwell.formats()[0]
    assert_type(info["kind"], Literal["text", "binary"])
    assert_type(info["options"], dict[str, Any])
    assert_type(info["extension"], str)
    problem: ValueError = InvalidTableError("x")
    problem = InvalidOptionsError("x")

    from gridwell.conformance import MISSING, Difference, Report, assert_conforms, check, normalize

    assert_type(normalize(t), dict[str, Any])
    assert_type(normalize("{}", "L1"), dict[str, Any])
    report = check({}, Path("grid.json"), "L2")
    assert_type(report, Report)
    assert_type(check(t, {"format": "x"}), Report)
    assert_type(assert_conforms(t, "grid.json", None), Report)
    assert_type(report.passed, bool)
    assert_type(report.tier, Literal["L0", "L1", "L2", "L3"])
    assert_type(report.tiers["L0"], Literal["pass", "fail", "not checked"])
    assert_type(report.errors, tuple[str, ...])
    assert_type(report.differences, tuple[Difference, ...])
    gone = report.differences[0].expected is MISSING
"""

GOOD_GT = """
    from typing import Any

    from great_tables import GT
    from typing_extensions import assert_type

    import gridwell
    from gridwell import Table
    from gridwell.conformance import Report, check, normalize

    def convert(gt: GT) -> None:
        assert_type(Table(gt), Table)
        assert_type(Table.from_gt(gt), Table)
        assert_type(gridwell.gt_to_ir(gt), str)
        assert_type(gridwell.gt_to_dict(gt), dict[str, Any])
        assert_type(normalize(gt), dict[str, Any])
        assert_type(check(gt, "grid.json"), Report)
        for emitter in ("auto", "upstream", "gridwell"):
            assert_type(gridwell.gt_to_ir(gt, emitter=emitter), str)
            assert_type(gridwell.gt_to_dict(gt, emitter), dict[str, Any])
"""

BAD = """
    from pathlib import Path
    from typing import Literal

    import gridwell
    from gridwell import Table

    t = Table("{}")
    t.render_html(inline_styles="yes")  # E
    t.render_html(bogus=1)  # E
    t.render_html(True)  # E
    t.render_rtf(color=True)  # E
    t.render_docx(anything=1)  # E
    t.render_latex(longtable_threshold=1.5)  # E
    t.render_ansi(max_width="80")  # E
    t.render_svg(font_size="16px")  # E
    t.render_quarto(extra_attrs=[("a", 1)])  # E
    t.render("html", ["inline_styles"])  # E
    t.render_binary(7)  # E
    t.warnings(1)  # E
    t.save(7)  # E
    html: bytes = t.render_html()  # E
    docx: str = t.render_docx()  # E
    name: str = t.save(Path("t.html"))  # E
    page: str = t._repr_html_()  # E
    kind: Literal["text"] = gridwell.formats()[0]["kind"]  # E
    gridwell.formats()[0]["nope"]  # E
    gridwell.parse_ir({})  # E
    Table.from_json({})  # E
    t.nope  # E

    from gridwell.conformance import check, normalize

    normalize(t, "L4")  # E
    check(t, "grid.json", "l1")  # E
    check(t, 7)  # E
    check(t, b"{}")  # E
    report = check(t, "grid.json")
    report.passed = False  # E
    flag: str = report.passed  # E
"""

# Without great_tables, `GT` is unknown (so anything could be one) and these pass.
BAD_GT = """
    from great_tables import GT

    import gridwell
    from gridwell import Table
    from gridwell.conformance import check, normalize

    Table(42)  # E
    Table.from_gt("gt")  # E
    gridwell.gt_to_ir("gt")  # E
    check(42, "grid.json")  # E
    normalize(b"{}")  # E

    def convert(gt: GT) -> None:
        gridwell.gt_to_ir(gt, emitter="great_tables")  # E
        gridwell.gt_to_dict(gt, emitter=None)  # E
"""


@pytest.mark.parametrize(
    "code,needs_gt",
    [(GOOD, False), (GOOD_GT, True), (BAD, False), (BAD_GT, True)],
    ids=["good", "good-gt", "bad", "bad-gt"],
)
def test_mypy_reads_the_stubs(tmp_path, code, needs_gt):
    need_mypy()
    if needs_gt and not HAS_GT:
        pytest.skip("great_tables isn't installed")
    lines, marked, output = run_mypy(tmp_path, code)
    assert lines == marked, output
