# gridwell

[![PyPI](https://img.shields.io/pypi/v/gridwell.svg)](https://pypi.org/project/gridwell/)
[![Python versions](https://img.shields.io/pypi/pyversions/gridwell.svg)](https://pypi.org/project/gridwell/)
[![CI](https://github.com/rich-iannone/gridwell/actions/workflows/ci.yml/badge.svg)](https://github.com/rich-iannone/gridwell/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Make a table once. Get it as HTML, LaTeX, Typst, RTF, Word, Excel, PowerPoint and
more.**

gridwell renders tables. Give it a [Great Tables](https://posit-dev.github.io/great-tables/)
table in Python or a [gt](https://gt.rstudio.com/) table in R, with its title,
formatting and styles, and it writes that same table to eleven formats: web pages,
LaTeX and Typst documents, RTF, SVG, the terminal, the Pandoc and Quarto ASTs, and
real Word, Excel and PowerPoint files. Titles stay titles, formatted numbers stay
formatted, and the Excel file holds numbers you can sum, not text.

Its engine is written in Rust, and the Python package, the R package and the
command-line tool all share it, so a table comes out the same whichever you use. Any
other tool can use it too, by describing its tables in gridwell's IR, a JSON format.

- **Documentation:** <https://rich-iannone.github.io/gridwell/>: a user guide, the
  IR's full schema and the API reference.
- **Gallery:** <https://rich-iannone.github.io/gridwell/gallery/>: every example table
  in every format, as each format's own viewer draws it.
- **Changes:** [CHANGELOG.md](CHANGELOG.md), and the notes of each
  [GitHub Release](https://github.com/rich-iannone/gridwell/releases).

> **Status: alpha.** The API may change between releases. The IR is frozen at 1.0:
> from here it only grows, by minor versions.

## Installation

### Python

```bash
pip install gridwell
pip install "gridwell[gt]"    # with the Great Tables integration
```

There are wheels for Python ≥ 3.10 on Linux, macOS and Windows. Elsewhere pip builds
gridwell from source, which needs a [Rust toolchain](https://rustup.rs/) (≥ 1.85).

### R

The R package isn't on CRAN yet. Install it from a clone of this repository (this
needs Rust ≥ 1.85):

```bash
git clone https://github.com/rich-iannone/gridwell.git
R CMD INSTALL gridwell/r-package
```

### Command line

```bash
cargo install --locked --git https://github.com/rich-iannone/gridwell gridwell-cli
```

### Rust

The crates aren't on crates.io yet, so depend on the repository:

```toml
[dependencies]
gridwell = { git = "https://github.com/rich-iannone/gridwell" }
```

## Usage

### Python, with Great Tables

Make a table with Great Tables as you always would, then hand it to `gridwell.Table`:

```python
import pandas as pd
from great_tables import GT
import gridwell

cities = pd.DataFrame({
    "city": ["Toronto", "Montréal", "Calgary", "Ottawa", "Edmonton"],
    "province": ["Ontario", "Quebec", "Alberta", "Ontario", "Alberta"],
    "population": [2794356, 1762949, 1306784, 1017449, 1010899],
    "area": [631.1, 364.7, 820.6, 2788.2, 765.6],
})

gt = (
    GT(cities, rowname_col="city")
    .tab_header(title="Canada's largest cities", subtitle="2021 census")
    .fmt_integer(columns="population")
    .fmt_number(columns="area", decimals=1)
    .cols_label(province="Province", population="Population", area="Area (km²)")
)

table = gridwell.Table(gt)

table.save("cities.docx")   # Word
table.save("cities.xlsx")   # Excel
table.save("cities.pptx")   # PowerPoint
table.save("cities.tex")    # LaTeX; the format comes from the extension
```

In a notebook, `table` shows as HTML. In a terminal, `print(table)` draws it:

```python
>>> print(table)
Canada's largest cities
2021 census
┌────────────┬────────────┬────────────┬────────────┐
│            │ Province   │ Population │ Area (km²) │
├────────────┼────────────┼────────────┼────────────┤
│ Toronto    │ Ontario    │  2,794,356 │      631.1 │
│ Montréal   │ Quebec     │  1,762,949 │      364.7 │
│ Calgary    │ Alberta    │  1,306,784 │      820.6 │
│ Ottawa     │ Ontario    │  1,017,449 │    2,788.2 │
│ Edmonton   │ Alberta    │  1,010,899 │      765.6 │
└────────────┴────────────┴────────────┴────────────┘
```

Text formats can also come back as strings, to put in a document of your own:

```python
>>> print(table.render_latex())
{\large\bfseries Canada's largest cities}\\
{\small 2021 census}\\[6pt]
\begin{tabular}{llrr}
\toprule
 & Province & Population & Area (km²) \\
\midrule
Toronto & Ontario & 2,794,356 & 631.1 \\
Montréal & Quebec & 1,762,949 & 364.7 \\
Calgary & Alberta & 1,306,784 & 820.6 \\
Ottawa & Ontario & 1,017,449 & 2,788.2 \\
Edmonton & Alberta & 1,010,899 & 765.6 \\
\bottomrule
\end{tabular}
<BLANKLINE>
```

### R, with gt

The R package works the same way with gt tables. Every `gw_*` function takes one:

```r
library(gt)
library(gridwell)

cities <- data.frame(
  city = c("Toronto", "Montréal", "Calgary", "Ottawa", "Edmonton"),
  province = c("Ontario", "Quebec", "Alberta", "Ontario", "Alberta"),
  population = c(2794356, 1762949, 1306784, 1017449, 1010899),
  area = c(631.1, 364.7, 820.6, 2788.2, 765.6)
)

gt_cities <- gt(cities, rowname_col = "city") |>
  tab_header(title = "Canada's largest cities", subtitle = "2021 census") |>
  fmt_integer(columns = population) |>
  fmt_number(columns = area, decimals = 1) |>
  cols_label(province = "Province", population = "Population", area = "Area (km²)")

gw_save(gt_cities, "cities.docx")
gw_save(gt_cities, "cities.xlsx")
html <- gw_render_html(gt_cities)
```

`as_gw_table()` converts a gt table once, and the result prints as text in the
console:

```r
as_gw_table(gt_cities)
#> <gw_table: 4 columns, 5 rows, titled "Canada's largest cities">
#>
#> Canada's largest cities
#> 2021 census
#> ┌────────────┬────────────┬────────────┬────────────┐
#> │            │ Province   │ Population │ Area (km²) │
#> ├────────────┼────────────┼────────────┼────────────┤
#> │ Toronto    │ Ontario    │  2,794,356 │      631.1 │
#> │ Montréal   │ Quebec     │  1,762,949 │      364.7 │
#> │ Calgary    │ Alberta    │  1,306,784 │      820.6 │
#> │ Ottawa     │ Ontario    │  1,017,449 │    2,788.2 │
#> │ Edmonton   │ Alberta    │  1,010,899 │      765.6 │
#> └────────────┴────────────┴────────────┴────────────┘
```

In R Markdown and Quarto, a `gw_table` that a chunk returns is shown in the
document's own format: HTML, LaTeX (PDF) or Typst.

### The command line

The `gridwell` command converts a table's IR, such as
[`hello.json`](docs/examples/hello.json), to any format:

```bash
$ gridwell convert hello.json --to typst
#table(
  columns: (auto, auto,),
  align: (left, right,),
  stroke: none,
  table.header(
    [#text(weight: "bold")[Name]],
    [#text(weight: "bold")[Score]],
    table.hline(stroke: 0.5pt),
  ),
  [Alice],
  [95],
  [Bob],
  [87],
)
```

```bash
gridwell convert hello.json -o hello.docx   # the format comes from the extension
gridwell validate hello.json                # check the IR
gridwell formats                            # list the formats and their options
```

### Rust

```rust
let json = std::fs::read_to_string("table.json")?;
let table = gridwell::Table::from_json(&json)?;

// By format name, with the format's options as JSON
let html = gridwell::render(&table, "html", None)?;

// Straight to a file, the format from the extension
gridwell::save(&table, std::path::Path::new("table.docx"), None, None)?;
```

Each format is a cargo feature, all on by default, so you can build only the formats
you need.

### From any other tool

gridwell's IR is a JSON description of a finished table: its cells' text, already
formatted, with the table's spans, styles, title and notes. A tool that writes it gets
every format gridwell has. The [IR schema](https://rich-iannone.github.io/gridwell/schema/)
describes it, with a [JSON Schema](https://rich-iannone.github.io/gridwell/schema/ir-1.0.schema.json)
(`gridwell schema` prints it) to check IR against in any language, and the
[conformance kit](conformance/README.md) checks an emitter's IR against the tables it
should make.

## Formats

| Format | Name | Extension |
|---|---|---|
| HTML | `html` | `.html` |
| LaTeX | `latex` | `.tex` |
| Typst | `typst` | `.typ` |
| RTF | `rtf` | `.rtf` |
| SVG | `svg` | `.svg` |
| Terminal text | `ansi` | `.txt` |
| Pandoc AST | `pandoc` | `.json` |
| Quarto | `quarto` | `.json` |
| Word | `docx` | `.docx` |
| Excel | `xlsx` | `.xlsx` |
| PowerPoint | `pptx` | `.pptx` |

Saving to a file takes the format from the extension, except for `.json`, which could
be Pandoc or Quarto: name the format for those.

SVG, ANSI, Pandoc and Quarto are experimental for now: their output is
valid and has every cell, but their styling is best-effort and may change between
releases.

Each format has options, such as the HTML writer's CSS class prefix or whether the
Excel writer adds its base styles. The [writer options](https://rich-iannone.github.io/gridwell/guide/writer-options.html)
page lists them, and the [feature matrix](https://rich-iannone.github.io/gridwell/guide/feature-matrix.html)
shows what each format can express.

## How it works

gridwell draws a firm line between *deciding* how a table looks and *rendering* it:

- **The table tool decides**: the data, the number and date formatting, the locale,
  and which styles go on which cells. It writes all of that into the IR.
- **gridwell renders** that IR, as faithfully as each format allows. It doesn't
  reformat values or interpret the data.

The one deliberate extra is a cell's typed value: the raw number, date or boolean
behind its formatted text, so that Excel cells can hold real values while every other
format shows the formatted text.

```
  Great Tables (Python) · gt (R) · your tool
                    │
                    │  IR (JSON)
                    ▼
┌──────────────────────────────────────────────┐
│              gridwell (Rust)                 │
│                                              │
│      parse ──▶ validate ──▶ lay out          │
│                                │             │
│                                ▼             │
│        one writer per format: html latex     │
│        typst rtf svg ansi pandoc quarto      │
│        docx xlsx pptx                        │
│                                              │
│     Python · R · CLI · C ABI · Rust crate    │
└──────────────────────────────────────────────┘
                    │
                    ▼
       HTML, PDF, Word, Excel, PowerPoint, …
```

## Development

The repository holds the Rust workspace, the Python and R packages and the docs:

| Path | What's there |
|---|---|
| `crates/gridwell/` | the crate to depend on, with a cargo feature per format |
| `crates/gridwell-ir/` | the IR's types, its JSON (de)serialization and the validator |
| `crates/gridwell-layout/` | the resolved grid, spans and styles every writer renders |
| `crates/gridwell-writer-*/` | one crate per format |
| `crates/gridwell-cli/` | the `gridwell` command |
| `crates/gridwell-python/`, `python/` | the Python package (PyO3, built with maturin) |
| `r-package/` | the R package (extendr) |
| `crates/gridwell-ffi/` | the C ABI |
| `crates/gridwell-testkit/` | the example tables every test uses, and a builder for the IR |
| `conformance/` | the conformance kit for IR emitters |
| `docs/` | the documentation site (Quarto) |
| `harness/`, `xtask/` | the visual test harness and the docs checks |

```bash
cargo test                     # every crate, with each writer's snapshots
pip install maturin && maturin develop --release && pytest
cargo xtask gallery            # render every example to every format
open harness/gallery/index.html
```

Every writer renders every example table to a committed snapshot, and a visual gate
compares the rendered images of each format with committed goldens. The
[harness README](harness/README.md) has the details, and what CI runs.

## License

MIT. See [LICENSE](LICENSE).
