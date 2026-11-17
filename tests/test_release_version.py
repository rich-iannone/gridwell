"""The release workflows' version check (.github/scripts/release_version.py)."""

import importlib.util
import shutil
from pathlib import Path

import pytest

ROOT = Path(__file__).parent.parent
SCRIPT = ROOT / ".github" / "scripts" / "release_version.py"

pytestmark = [
    pytest.mark.skipif(not SCRIPT.is_file(), reason="no checkout here"),
    # The script runs on the CI runner's Python, which has tomllib (3.11+).
    pytest.mark.skipif(importlib.util.find_spec("tomllib") is None, reason="needs tomllib"),
]


def script():
    spec = importlib.util.spec_from_file_location("release_version", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def copy(tmp_path):
    """The three files that hold the version, and the changelog, in a scratch root."""
    for rel in ("Cargo.toml", "pyproject.toml", "python/gridwell/__init__.py", "CHANGELOG.md"):
        (tmp_path / rel).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(ROOT / rel, tmp_path / rel)
    return tmp_path


def test_the_versions_agree(capsys, monkeypatch, tmp_path):
    monkeypatch.setenv("GITHUB_OUTPUT", str(tmp_path / "out"))
    rv = script()
    version = rv.versions(ROOT)["Cargo.toml"]
    assert rv.main([]) == 0, capsys.readouterr().out
    assert capsys.readouterr().out == f"{version}\n"
    assert (tmp_path / "out").read_text() == f"version={version}\n"
    assert rv.main(["--tag", f"py-v{version}"]) == 0


@pytest.mark.parametrize("tag", ["v{v}", "py-v{v}.1", "py-{v}", "r-v{v}"])
def test_a_tag_must_be_py_v_and_the_version(tag, capsys, monkeypatch):
    monkeypatch.delenv("GITHUB_OUTPUT", raising=False)
    rv = script()
    tag = tag.format(v=rv.versions(ROOT)["Cargo.toml"])
    assert rv.main(["--tag", tag]) == 1
    assert capsys.readouterr().out.startswith(f"::error::tag {tag} does not match")


@pytest.mark.parametrize(
    "rel, old, new",
    [
        ("Cargo.toml", '[workspace.package]\nversion = "', '[workspace.package]\nversion = "9'),
        ("pyproject.toml", '\nversion = "', '\nversion = "9'),
        ("python/gridwell/__init__.py", '__version__ = "', '__version__ = "9'),
        ("python/gridwell/__init__.py", "__version__ = ", "__version = "),
    ],
)
def test_any_version_that_differs_fails(rel, old, new, tmp_path, capsys, monkeypatch):
    monkeypatch.setenv("GITHUB_OUTPUT", str(tmp_path / "out"))
    root = copy(tmp_path / "root")
    text = (root / rel).read_text(encoding="utf-8")
    assert text.count(old) == 1, (rel, old)
    (root / rel).write_text(text.replace(old, new), encoding="utf-8")
    rv = script()
    assert rv.main([], root) == 1
    out = capsys.readouterr().out
    assert out.startswith("::error::the versions differ:") and rel in out, out
    assert not (tmp_path / "out").exists()


def changelog(root, text):
    (root / "CHANGELOG.md").write_text(text, encoding="utf-8")
    return script().versions(root)["Cargo.toml"]


def test_the_changelog_has_the_version(tmp_path, capsys):
    rv = script()
    version = rv.versions(ROOT)["Cargo.toml"]
    section = rv.notes(ROOT, version)
    assert section and not section.startswith("#"), section
    out = tmp_path / "notes.md"
    assert rv.main(["--notes", str(out)]) == 0, capsys.readouterr().out
    assert out.read_text(encoding="utf-8") == section


@pytest.mark.parametrize("heading", ["## {v}", "## {v} (2026-11-20)", "## {v}  "])
def test_notes_are_the_versions_section(heading, tmp_path, capsys):
    root = copy(tmp_path / "root")
    v = script().versions(root)["Cargo.toml"]
    changelog(
        root,
        f"# Changelog\n\nIntro.\n\n## 9.9.9\n\nLater.\n\n{heading.format(v=v)}\n\n"
        f"First.\n\n### Part\n\n- one\n\n## 0.0.1\n\nEarlier.\n",
    )
    out = tmp_path / "notes.md"
    assert script().main(["--notes", str(out)], root) == 0, capsys.readouterr().out
    assert out.read_text(encoding="utf-8") == "First.\n\n### Part\n\n- one\n"


@pytest.mark.parametrize(
    "text",
    [
        None,
        "# Changelog\n",
        "# Changelog\n\n## {v}\n\n## 0.0.1\n\nEarlier.\n",
        "# Changelog\n\n## {v}\n   \n",
        "# Changelog\n\n## {v}.1\n\nNot this one.\n",
        "# Changelog\n\n## v{v}\n\nNot this one.\n",
        "# Changelog\n\n### {v}\n\nNot a section.\n",
        "# Changelog\n\n##{v}\n\nNot a heading.\n",
    ],
)
def test_no_section_for_the_version_fails(text, tmp_path, capsys, monkeypatch):
    monkeypatch.setenv("GITHUB_OUTPUT", str(tmp_path / "out"))
    root = copy(tmp_path / "root")
    if text is None:
        (root / "CHANGELOG.md").unlink()
    else:
        changelog(root, text.format(v=script().versions(root)["Cargo.toml"]))
    notes = tmp_path / "notes.md"
    assert script().main(["--notes", str(notes)], root) == 1
    assert capsys.readouterr().out.startswith("::error::CHANGELOG.md has no section for")
    assert not (tmp_path / "out").exists()
    assert not notes.exists()
