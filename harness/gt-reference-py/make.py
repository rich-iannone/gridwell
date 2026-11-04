"""Build the Great Tables reference set for the "Great Tables vs gridwell" gallery page.

For each case in cases.py this writes, into this directory:

    <name>.gt.html   Great Tables' own HTML (`as_raw_html()`), the look gridwell's
                     HTML is measured against
    <name>.ir.json   the same table as gridwell IR (`gridwell.gt_to_ir()`: Great
                     Tables' own `GT.as_ir()` when the installed Great Tables has
                     it, else gridwell's copy)

and manifest.json (case descriptions, the Python, Great Tables and gridwell versions
used, and which emitter wrote the IR).

The files are checked in so the visual harness (which has no Python) can
rasterize both sides. Regenerate after changing the Great Tables emitter or
upgrading Great Tables, with the development gridwell installed in an
environment that has great_tables and pandas:

    maturin develop --release        # or install a freshly built wheel
    python harness/gt-reference-py/make.py

Then run `cargo xtask gt` and open harness/gallery/great-tables.html.
"""

from __future__ import annotations

import importlib.metadata
import importlib.util
import json
import platform
import sys
from pathlib import Path

import gridwell
from gridwell._gt import has_upstream

OUT_DIR = Path(__file__).resolve().parent


def load_cases() -> dict:
    """The cases in cases.py, imported by path (this directory isn't a package)."""
    sys.dont_write_bytecode = True  # no __pycache__ in the source tree
    spec = importlib.util.spec_from_file_location("gt_reference_cases", OUT_DIR / "cases.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.cases


def write(path: Path, text: str) -> None:
    # As R's writeLines() does for the gt references: UTF-8, LF, a final newline.
    path.write_text(text + "\n", encoding="utf-8", newline="\n")


def main() -> None:
    cases = load_cases()
    manifest_cases = []
    emitters = set()
    for name, case in cases.items():
        tbl = case["build"]()
        # Great Tables' own emitter when there is one, so the references follow it.
        emitter = "upstream" if has_upstream(tbl) else "gridwell"
        emitters.add("great_tables" if emitter == "upstream" else "gridwell")
        write(OUT_DIR / f"{name}.gt.html", tbl.as_raw_html())
        write(OUT_DIR / f"{name}.ir.json", gridwell.gt_to_ir(tbl, emitter=emitter))
        manifest_cases.append({"name": name, "description": case["description"]})
        print(f"wrote {name}")

    manifest = {
        "generated_by": "harness/gt-reference-py/make.py",
        "python_version": platform.python_version(),
        "great_tables_version": importlib.metadata.version("great_tables"),
        "gridwell_version": importlib.metadata.version("gridwell"),
        "ir_emitter": emitters.pop(),
        "cases": manifest_cases,
    }
    write(OUT_DIR / "manifest.json", json.dumps(manifest, indent=2, ensure_ascii=False))
    print(f"wrote manifest.json ({len(cases)} cases)")


if __name__ == "__main__":
    main()
