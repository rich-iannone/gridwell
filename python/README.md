# gridwell

Render one table to HTML, LaTeX, Typst, RTF, SVG, terminal text, Pandoc and Quarto,
Word, Excel and PowerPoint. gridwell takes a [Great Tables](https://posit-dev.github.io/great-tables/)
table as it is, or its own IR, a JSON description of a fully formatted table that
any tool can write.

gridwell's engine is written in Rust. This is the Python package; an R package built
on the same engine takes gt tables. Source, issues and the R package are at
[github.com/rich-iannone/gridwell](https://github.com/rich-iannone/gridwell). The
documentation, with a user guide and the IR's full schema, is at
[rich-iannone.github.io/gridwell](https://rich-iannone.github.io/gridwell/).

> **Status: alpha.** The API may change between releases.

## Installation

```bash
pip install gridwell
pip install "gridwell[gt]"    # with the Great Tables integration
```

`import gridwell` needs nothing else; the `gt` extra adds pandas and Great Tables 1.1.0 or
newer.

Wheels cover Python ≥ 3.10 on Linux (x86_64, aarch64), macOS (Intel, Apple silicon)
and Windows (x86_64). Elsewhere, and on free-threaded Python builds, pip builds from
the source distribution, which needs a Rust toolchain (≥ 1.85).

## Usage

A Great Tables table goes straight in:

```python
from great_tables import GT, exibble
import gridwell

table = gridwell.Table(GT(exibble.head(5), rowname_col="row", groupname_col="group"))

html = table.render_html()
latex = table.render_latex()
table.save("table.docx")  # the format comes from the extension
```

In a notebook a `Table` shows as HTML; `print(table)` draws it as text.

Any tool can make a table by writing the IR, as JSON or a dict. Most fields can be
left out, taking their defaults:

```python
import gridwell

def text(value):
    return {"content": [{"type": "text", "value": value}]}

table = gridwell.Table({
    "ir_version": "1.0",
    "config": {"table_cols": 2, "header_rows": 1, "body_rows": 2},
    "styles": {},
    "column_spec": [{"id": "name"}, {"id": "score", "align": "right"}],
    "table": {
        "thead": {"rows": [{"cells": [text("Name"), text("Score")]}]},
        "tbody": [{"rows": [
            {"cells": [text("Alice"), text("95")]},
            {"cells": [text("Bob"), text("87")]},
        ]}],
    },
})
```

```python
>>> print(table.render("latex"))
\begin{tabular}{lr}
\toprule
Name & Score \\
\midrule
Alice & 95 \\
Bob & 87 \\
\bottomrule
\end{tabular}
<BLANKLINE>
>>> print(table)
┌────────────┬────────────┐
│ Name       │      Score │
├────────────┼────────────┤
│ Alice      │         95 │
│ Bob        │         87 │
└────────────┴────────────┘
```

## Formats

| Format | Name | Returns |
|---|---|---|
| HTML | `html` | `str` |
| LaTeX | `latex` | `str` |
| Typst | `typst` | `str` |
| RTF | `rtf` | `str` |
| SVG | `svg` | `str` |
| Terminal text | `ansi` | `str` |
| Pandoc AST | `pandoc` | `str` (JSON) |
| Quarto | `quarto` | `str` (JSON) |
| Word | `docx` | `bytes` |
| Excel | `xlsx` | `bytes` |
| PowerPoint | `pptx` | `bytes` |

`table.render(name)` gives the text formats and `table.render_binary(name)` the Office
ones. Each format also has a method with its options as keyword arguments, such as
`table.render_xlsx(base_styles=False)`. `gridwell.formats()` lists the formats with
their options.

SVG, ANSI, Pandoc and Quarto are experimental for now: their output is valid and has
every cell, but their styling is best-effort and may change between releases.

## License

MIT.
