# Changelog

What changed in each gridwell release. The Python package is released to PyPI on a
`py-vX.Y.Z` tag, and each release's notes on GitHub are its section here.

## 0.1.0

The first release: the Python package, on PyPI. The R package, the `gridwell`
command and the Rust crates are at the same version, and install from the
repository until they are published.

### The IR

- IR 1.0, gridwell's JSON description of a finished table, is frozen: every later
  gridwell that reads IR 1 reads it the same, and IR 1 only grows, by minor
  versions. Great Tables' own converter, `GT.as_gridwell()`, writes it (on Great
  Tables' main branch, not yet in a release).
- A [JSON Schema](https://rich-iannone.github.io/gridwell/schema/ir-1.0.schema.json)
  for IR 1.0, to check IR in any language (`gridwell schema` prints it).
- Validation names each problem with a rule code. A table of a newer minor version
  is read, with a warning, without what that version added; another major version is
  refused.

### Formats

- Eleven formats: HTML, LaTeX, Typst, RTF, Word, Excel and PowerPoint, and, as
  experimental formats (their styling is best-effort and may change between
  releases), SVG, terminal text, the Pandoc AST and Quarto.
- Excel cells hold the number, date or boolean behind a cell's formatted text, so
  they can be summed and sorted.
- Each format has options (see
  [Writer Options](https://rich-iannone.github.io/gridwell/guide/writer-options.html)),
  and warns about what it can't express.

### Python

- `gridwell.Table` takes IR (as JSON or a dict) or a Great Tables `GT`, renders it
  with `render_html()`, `render_docx()` and the rest, and saves it with `save()`,
  which takes the format from the file's extension. In a notebook a table shows as
  HTML, and `print()` draws it as text.
- The Great Tables integration (`pip install "gridwell[gt]"`, Great Tables 1.1.0 or
  newer): `Table(gt)`, `gt_to_ir()` and `gt_to_dict()`, for tables on pandas or
  polars. They use Great Tables' own converter when the installed Great Tables has
  one, and gridwell's otherwise.
- `gridwell.conformance` checks an emitter's IR against the tables it should make.
- Type stubs for every function and each format's options. The work done in Rust
  releases the GIL.
- Wheels for Python 3.10 and later on Linux (x86_64, aarch64), macOS (Intel, Apple
  silicon) and Windows (x86_64).

### Also in the repository

- The R package: the same for gt tables (`gw_render_html()`, `gw_save()` and the
  rest, `as_gw_table()`), and in R Markdown and Quarto documents.
- The `gridwell` command: `convert`, `validate`, `fmt`, `inspect`, `grid`,
  `formats`, `conformance` and `schema`.
- The Rust crate `gridwell`, with a cargo feature per format, and a C API.

### Known issues

- In LibreOffice, a right-to-left table in RTF or Word sits at the left margin
  under its right-aligned title.
