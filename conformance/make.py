"""Write the expected grids in conformance/expected/ from the reference sets.

For each case (in the gt reference set's manifest) this writes:

    expected/gt/<name>.json            to L3: the normalized grid of the gt
                                       reference IR (harness/gt-reference), after
                                       checking its L1 against gt's own HTML
    expected/great_tables/<name>.json  to L3: the same, from the Great Tables
                                       reference set (harness/gt-reference-py)

The L1 of both is checked against the package's own HTML (read by html_grid.py),
so it doesn't rest on trusting either emitter. L2 and L3 can't be read off HTML:
they are those of the reviewed reference IR.

It reads only checked-in files, so it needs neither R nor Great Tables: only a
gridwell with `gridwell.conformance`. Run it from anywhere after regenerating a
reference set (or changing the normalizer):

    python conformance/make.py           # write
    python conformance/make.py --check   # exit 1 if a file would change
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

sys.dont_write_bytecode = True  # no __pycache__ in the source tree

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import html_grid  # noqa: E402

from gridwell.conformance import normalize  # noqa: E402

ROOT = HERE.parent
EXPECTED = HERE / "expected"
GT_REF = ROOT / "harness" / "gt-reference"
PY_REF = ROOT / "harness" / "gt-reference-py"
# Each package's possessive, for messages.
PACKAGES = {"gt": "gt's", "great_tables": "Great Tables'"}


def manifest(ref: Path) -> dict:
    return json.loads((ref / "manifest.json").read_text(encoding="utf-8"))


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def dump(obj: dict) -> str:
    return json.dumps(obj, indent=2, ensure_ascii=False) + "\n"


def expected_grid(name: str, ref: Path, package: str, version: str, m: dict) -> dict:
    """A case's grid to L3 from a reference set's IR, after checking that its L1
    is what the package's HTML shows."""
    ir = ref / f"{name}.ir.json"
    html = ref / f"{name}.gt.html"
    grid = normalize(ir.read_text(encoding="utf-8"), "L3")
    seen = html_grid.grid(html.read_text(encoding="utf-8"))["grid"]
    l1 = normalize(ir.read_text(encoding="utf-8"), "L1")["grid"]
    if l1 != seen:
        raise SystemExit(
            f"{name}: the reference IR's L1 grid isn't what {PACKAGES[package]} HTML "
            f"shows; compare `gridwell conformance normalize --tier L1 {rel(ir)}` with "
            f"`python conformance/html_grid.py {rel(html)}`"
        )
    return {
        "format": grid["format"],
        "tier": grid["tier"],
        "case": name,
        "producer": {"package": package, "version": version},
        "source": {
            "L1": f"{rel(html)} ({PACKAGES[package]} HTML)",
            "L2, L3": f"{rel(ir)} ({m['ir_emitter']}'s emitter, reviewed)",
        },
        "grid": grid["grid"],
    }


def build() -> dict[Path, str]:
    """Every expected file and its text."""
    gt, py = manifest(GT_REF), manifest(PY_REF)
    names = [c["name"] for c in gt["cases"]]
    if names != [c["name"] for c in py["cases"]]:
        raise SystemExit("the gt and Great Tables reference sets have different cases")
    files = {}
    for name in names:
        files[EXPECTED / "gt" / f"{name}.json"] = dump(
            expected_grid(name, GT_REF, "gt", gt["gt_version"], gt)
        )
        files[EXPECTED / "great_tables" / f"{name}.json"] = dump(
            expected_grid(name, PY_REF, "great_tables", py["great_tables_version"], py)
        )
    return files


def main(argv: list[str]) -> int:
    check = argv == ["--check"]
    if argv and not check:
        print(__doc__, file=sys.stderr)
        return 2
    files = build()
    stale = [
        p
        for p in sorted(set(files) | set(EXPECTED.glob("*/*.json")))
        if p not in files or not p.exists() or p.read_text(encoding="utf-8") != files[p]
    ]
    if check:
        for p in stale:
            print(f"out of date: {rel(p)}")
        return 1 if stale else 0
    for p in stale:
        if p in files:
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(files[p], encoding="utf-8", newline="\n")
            print(f"wrote {rel(p)}")
        else:
            p.unlink()
            print(f"removed {rel(p)}")
    print(f"{len(files)} expected grids, {len(stale)} changed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
