"""The docs' Python blocks run.

Every ```python block on a docs page (docs/**/*.qmd) or a README runs, the blocks of
a page in order and sharing one namespace, as a notebook would. Blocks that start
with `>>>` run as doctests, so what they show must be what Python prints. The
namespace starts with what the pages take as given: `json_str` (the getting-started
table's IR), `ir_dict` (the same as a dict), `bad_json` (IR that doesn't parse),
`table` (the table), `ir` (the IR of the conformance kit's `exibble_basic` case)
and, with the `gt` extra, `gt` (a Great Tables table). Without the extra, a page
with a block that uses Great Tables is skipped, as its other blocks may build on
it. The blocks run in a scratch directory that has an `out/` directory and the
repository's `conformance/expected/`.

`<!-- docs-check: skip -->` on the line before a fence (blank lines aside) leaves
the block out, for code meant to fail (such as type errors).
"""

import doctest
import importlib.util
import json
import re
import shutil
import traceback
from pathlib import Path

import pytest

import gridwell

ROOT = Path(__file__).parent.parent
DOCS = ROOT / "docs"
READMES = [
    "README.md",
    "crates/gridwell/README.md",
    "crates/gridwell-cli/README.md",
    "conformance/README.md",
]
SKIP = "<!-- docs-check: skip -->"
NEEDS_GT = re.compile(r"\b(gt|GT|great_tables|exibble)\b")
HAS_GT = all(importlib.util.find_spec(m) for m in ("great_tables", "pandas"))


def pages():
    """The pages, or none where the docs aren't (an installed package's tests)."""
    if not DOCS.is_dir():
        return []
    found = [
        p
        for p in DOCS.rglob("*.qmd")
        if "_site" not in p.parts and ".quarto" not in p.parts
    ]
    found += [ROOT / r for r in READMES]
    return sorted(found)


def is_python(info):
    return info == "python" or info.startswith(("{.python}", "{.python "))


def blocks(text):
    """The page's Python blocks, as (line of the opening fence, code)."""
    out = []
    open_ = None  # (line, backtick count, kept, code lines)
    skip_next = False
    for i, line in enumerate(text.splitlines(), start=1):
        t = line.rstrip()
        if open_ is None:
            if t.startswith("```"):
                info = t.lstrip("`")
                keep = is_python(info.strip()) and not skip_next
                open_ = (i, len(t) - len(info), keep, [])
                skip_next = False
            elif t.strip() == SKIP:
                skip_next = True
            elif t.strip():
                skip_next = False
        elif len(t) >= open_[1] and set(t) == {"`"}:
            if open_[2]:
                out.append((open_[0], "\n".join(open_[3]) + "\n"))
            open_ = None
        else:
            open_[3].append(line)
    return out


def namespace(gt):
    json_str = (DOCS / "examples" / "hello.json").read_text(encoding="utf-8")
    ns = {
        "json_str": json_str,
        "ir_dict": json.loads(json_str),
        "bad_json": '{"ir_version": ',
        "table": gridwell.Table(json_str),
        "ir": (ROOT / "harness/gt-reference-py/exibble_basic.ir.json").read_text(
            encoding="utf-8"
        ),
        "gridwell": gridwell,
    }
    if gt:
        from great_tables import GT, exibble

        ns["gt"] = GT(exibble.head(5))
    return ns


def needs_gt(text):
    return any(NEEDS_GT.search(code) for _, code in blocks(text))


def run_page(name, text, ns):
    """Run the page's blocks in `ns`; returns the failures."""
    failures = []
    for line, code in blocks(text):
        where = f"{name}:{line}"
        if code.lstrip().startswith(">>>"):
            test = doctest.DocTestParser().get_doctest(code, ns, where, name, line)
            report = []
            runner = doctest.DocTestRunner(verbose=False)
            if runner.run(test, out=report.append, clear_globs=False).failed:
                failures.append(f"{where}:\n{''.join(report)}")
        else:
            try:
                # Pad so tracebacks give the page's own line numbers.
                exec(compile("\n" * line + code, name, "exec"), ns)
            except Exception:
                failures.append(f"{where}:\n{traceback.format_exc()}")
    return failures


def rel(path):
    return path.relative_to(ROOT).as_posix()


PAGES = [p for p in pages() if blocks(p.read_text(encoding="utf-8"))]


@pytest.mark.parametrize("page", PAGES, ids=[rel(p) for p in PAGES])
def test_python_blocks_run(page, tmp_path, monkeypatch):
    (tmp_path / "out").mkdir()
    # A copy, not a link: Windows needs privileges for links.
    shutil.copytree(
        ROOT / "conformance" / "expected", tmp_path / "conformance" / "expected"
    )
    monkeypatch.chdir(tmp_path)
    text = page.read_text(encoding="utf-8")
    if needs_gt(text) and not HAS_GT:
        pytest.skip("needs the gt extra")
    failures = run_page(rel(page), text, namespace(HAS_GT))
    assert not failures, "\n".join(failures)


def test_the_pages_have_python_blocks():
    if not DOCS.is_dir():
        pytest.skip("no docs here")
    count = sum(len(blocks(p.read_text(encoding="utf-8"))) for p in pages())
    # The pages hold this many; fewer means the extraction lost some.
    assert count >= 23, count


def test_the_check_catches_wrong_blocks():
    """Planted pages: each wrong block fails, and skipped ones don't run."""
    ns = lambda: {"table": gridwell.Table(namespace(False)["json_str"])}  # noqa: E731
    right = (
        "```python\nx = table.render('latex')\n```\n\n"
        "```{.python filename='a.py'}\nassert x.startswith('\\\\begin')\n```\n\n"
        ">>> not a block\n\n"
        "```python\n>>> 1 + 1\n2\n```\n"
    )
    assert blocks(right)[1][0] == 5
    assert run_page("p.qmd", right, ns()) == []

    wrong = {
        "an error": "```python\ntable.render('htm')\n```\n",
        "a name": "```python\nprint(undefined)\n```\n",
        "a failed assert": "```python\nassert table.validate() == ['x']\n```\n",
        "another output": "```python\n>>> 1 + 1\n3\n```\n",
        "a later block": "```python\ny = 1\n```\n\n```python\n>>> y\n2\n```\n",
    }
    for what, page in wrong.items():
        failures = run_page("p.qmd", page, ns())
        assert len(failures) == 1, (what, failures)
    # Tracebacks give the page's line.
    failures = run_page("p.qmd", "Text.\n\n```python\n\nprint(undefined)\n```\n", ns())
    assert 'File "p.qmd", line 5' in failures[0], failures[0]

    left_out = (
        f"{SKIP}\n\n```python\nraise SystemError\n```\n\n"
        "```bash\nexit 1\n```\n\n"
        "````markdown\n```python\nraise SystemError\n```\n````\n"
    )
    assert run_page("p.qmd", left_out, ns()) == []
    assert not needs_gt(left_out)
    assert needs_gt(left_out + "\n```python\ngt.tab_header('T')\n```\n")
    assert not needs_gt("```python\ngt_x = 1\n```\n")
    # The marker covers one fence only.
    assert len(blocks(f"{SKIP}\n\nText.\n\n```python\nx\n```\n")) == 1
