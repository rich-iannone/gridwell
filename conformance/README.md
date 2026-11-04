# Conformance kit

The table IR is written by emitters gridwell doesn't own: gt's `as_ir()` (R) and
Great Tables' `GT.as_ir()` (Python), with gridwell's own copies as a fallback
until those ship. This directory is what an emitter's tests check it against:
a set of tables, and for each, what its IR must say.

Two correct emitters can write the same table very differently: other style ids,
a composition where the other has a definition, `red` or `#FF0000`, `bold` or
`700`, keys in another order, a text run split in two. So IR is compared as a
**normalized grid**: what a reader sees, with every style its resolved
properties, text as merged runs, footnote marks by footnote number, and covered
span slots gone.

## Tiers

Each tier includes the ones before it; a table passes a tier when nothing differs
at it or below.

| Tier | Checks |
|------|--------|
| L0 valid | The IR parses and validates. |
| L1 structure | Columns and stub, title, subtitle and extra lines, head rows and their spans, groups and their labels, row roles, data and summary rows, the text of every cell, footnote and source note. |
| L2 content | Text runs, footnote marks, images and raw content, typed values, `data_type`, `sort_key`, `scope`, column and group ids, `lang` and `dir`. |
| L3 styling | Resolved styles, alignment, column, table and image sizes. |
| L4 visual | Not here: the gt and Great Tables gallery pages (`cargo xtask gt`, see `harness/README.md`), report only. |

An expected grid says which tier it goes to, and is checked to that tier unless
asked for less.

## Layout

```
conformance/
  cases.R, cases.py      the tables, in gt and in Great Tables (the same names
                         and tables; each mirrors the other)
  expected/gt/           a grid per case, to L3, for gt
  expected/great_tables/ a grid per case, to L1, for Great Tables
  html_grid.py           the L1 grid of a table as gt or Great Tables draws it,
                         read from its own HTML
  make.py                writes expected/ from the reference sets
```

The reference sets the expected grids are made from are in `harness/gt-reference`
(gt) and `harness/gt-reference-py` (Great Tables): each case's HTML from the
package and its IR.

Where each tier's expectation comes from (each file's `source` says it too):

- **L1** is read off the package's own HTML (`as_raw_html()`) by `html_grid.py`,
  so it doesn't rest on trusting any emitter. For gt, `make.py` also checks that
  the reference IR's L1 is the same and refuses to write if it isn't.
- **L2 and L3** can't be read off HTML. For gt they are the normalized grid of
  the reference IR, which comes from gridwell's emitter (`ir_emitter` in
  `harness/gt-reference/manifest.json`) and has been reviewed; they say what that
  emitter does, for example that a date column is `data_type: "string"`. Great
  Tables has none yet.

## Checking an emitter

In Python (`gridwell.conformance`); `ir` is IR as a JSON string or a dict, a
`gridwell.Table`, or a `GT`:

```python
from gridwell.conformance import assert_conforms, check

assert_conforms(gt_table, "conformance/expected/great_tables/exibble_basic.json")

report = check(gt_table, "conformance/expected/great_tables/exibble_basic.json")
report.passed, report.tiers, report.differences
print(report)
```

In R, with a gt table, a `gw_table` or IR as a JSON string:

```r
report <- gridwell::gw_conformance_check(gt_tbl, "conformance/expected/gt/exibble_basic.json")
testthat::expect_true(report$passed, info = format(report))
```

From the command line (exit 1 when it doesn't conform; `--json` for the report as
JSON):

```bash
gridwell conformance check table.json conformance/expected/gt/exibble_basic.json
gridwell conformance check --tier L1 table.json conformance/expected/gt/exibble_basic.json
gridwell conformance normalize --tier L1 table.json   # the table's grid
```

A failing report lists every difference with its tier and where it is (here, IR
that formats the first number another way):

```
fails L1 (checked to L3)
  L0 valid: pass
  L1 structure: fail (1 difference)
  L2 content: fail (2 differences)
  L3 styling: fail
3 differences:
  L2 groups[0].rows[0].cells[0].runs[0].text: expected "1.111e-01", got "0.1111"
  L2 groups[0].rows[0].cells[0].sort_key: expected "1.111e-01", got "0.1111"
  L1 groups[0].rows[0].cells[0].text: expected "1.111e-01", got "0.1111"
```

An emitter checks the tables of `cases.R` or `cases.py`, built with the package
version the expected grids were made with (`producer.version` in each file):
another version can format values differently.

## Where the emitters stand

- **gt** (gridwell's emitter): every case passes `expected/gt` to L3, by
  construction (the R tests check that it stays so, live from `cases.R`).
- **Great Tables** (gridwell's emitter): every case passes `expected/great_tables`
  (L1); the Python tests check the reference IR and, with the Great Tables
  version in the manifest, the cases live from `cases.py`. They also check more
  tables (markup in labels, spanner levels, summary rows without a stub) against
  what `html_grid.py` reads off Great Tables' HTML. The emitter now places
  footnote marks in cells and styles each cell as `tab_style()` and the table
  options say, as the gt emitter does (the Python tests check both against
  Great Tables' HTML). L2 and L3 grids still wait on two gaps: stub cells in data
  rows have no `scope` (Great Tables writes `scope="row"`), and `fmt_*()` output
  that is HTML (`fmt_markdown()`, `fmt_image()`, nanoplots) is put in a cell as
  text. Grids made from the emitter now would say those gaps are right.

## Regenerating

After regenerating a reference set (`harness/gt-reference/make.R`,
`harness/gt-reference-py/make.py`) or changing the normalizer, with a gridwell
that has `gridwell.conformance` installed (no R or Great Tables needed):

```bash
python conformance/make.py           # write what changed
python conformance/make.py --check   # exit 1 if a file would change
```

The Python tests fail when the expected grids are out of date.

Adding a case: add it to `cases.R` and `cases.py`, run both reference make
scripts, then `conformance/make.py`, and check in the new files.

## The grid format

`{"format": "gridwell-conformance-grid/1", "tier": ..., "grid": {...}}`; other
top-level keys (`case`, `producer`, `source`) are for people. Each key of the grid
belongs to a tier (nested keys go to the highest along their path), and a grid
written to a tier has only the keys up to it; `gridwell conformance normalize`
shows one. Arrays are compared item by item and objects key by key, so a missing
or extra item or key is a difference; numbers compare by value.
