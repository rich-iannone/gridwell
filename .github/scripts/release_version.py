"""The version a Python release would publish, for the release workflows.

The version is written in three places, which must agree: the workspace version in
Cargo.toml (every crate's), the project version in pyproject.toml (the wheels') and
`__version__` in python/gridwell/__init__.py. With `--tag TAG`, TAG must also be
`py-v<version>`. Prints the version, and writes `version=<version>` to
`$GITHUB_OUTPUT` when it is set; on a mismatch, prints a GitHub error annotation
and exits 1.

CHANGELOG.md must have a section for the version (`## 0.1.0`), which isn't empty.
With `--notes FILE`, writes that section, without its heading, to FILE: the GitHub
Release's notes.

    python3 .github/scripts/release_version.py [--tag py-v0.1.0] [--notes notes.md]
"""

import argparse
import os
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def versions(root: Path) -> dict[str, str]:
    def toml(rel):
        with open(root / rel, "rb") as f:
            return tomllib.load(f)

    init = (root / "python/gridwell/__init__.py").read_text(encoding="utf-8")
    found = re.search(r'^__version__ = "([^"]*)"$', init, re.MULTILINE)
    return {
        "Cargo.toml": toml("Cargo.toml")["workspace"]["package"]["version"],
        "pyproject.toml": toml("pyproject.toml")["project"]["version"],
        "python/gridwell/__init__.py": found[1] if found else "(none)",
    }


def notes(root: Path, version: str) -> str | None:
    """The section of CHANGELOG.md for `version`, without its heading, or `None`."""
    path = root / "CHANGELOG.md"
    if not path.is_file():
        return None
    section = None
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("## "):
            if section is not None:
                break
            if line[3:].split(maxsplit=1)[:1] == [version]:
                section = []
        elif section is not None:
            section.append(line)
    text = "\n".join(section or []).strip()
    return text + "\n" if text else None


def main(argv: list[str], root: Path = ROOT) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tag", help="the release's tag, which must be py-v<version>")
    parser.add_argument("--notes", help="write the version's CHANGELOG.md section to this file")
    args = parser.parse_args(argv)

    found = versions(root)
    if len(set(found.values())) != 1:
        listed = ", ".join(f"{where} {v}" for where, v in found.items())
        print(f"::error::the versions differ: {listed}")
        return 1
    version = found["Cargo.toml"]
    if args.tag is not None and args.tag != f"py-v{version}":
        print(f"::error::tag {args.tag} does not match the version {version} (py-v{version})")
        return 1

    section = notes(root, version)
    if section is None:
        print(f"::error::CHANGELOG.md has no section for {version} (## {version}), or it is empty")
        return 1
    if args.notes is not None:
        Path(args.notes).write_text(section, encoding="utf-8")

    print(version)
    if out := os.environ.get("GITHUB_OUTPUT"):
        with open(out, "a", encoding="utf-8") as f:
            f.write(f"version={version}\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
