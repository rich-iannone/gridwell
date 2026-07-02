# gridwell

**Render publication-quality tables to any format from a single declarative IR.**

Gridwell is a Rust library (with a CLI, a C ABI, and Python/R bindings) that takes a
rich table **Intermediate Representation (IR)** and writes it to many output formats
with high fidelity. It's a *universal table-rendering backend*: any upstream tool can
emit the IR and get consistent output as HTML, LaTeX, Typst, RTF, SVG, terminal text,
Pandoc/Quarto AST, or Office documents (Word / Excel / PowerPoint).

The main idea underlying this is *decide once, render everywhere*. Upstream tools
already know how to build and format a table so they shouldn't each reimplement a
LaTeX or .docx writer. Gridwell is envisioned as being the shared rendering layer that
turns one description into every feasible format for tables.

## Goals

1. **Single source of truth**: one IR captures everything needed to render a table.
2. **High-fidelity output**: each writer targets the full capability of its format
rather than a lowest-common-denominator.
3. **Extensibility**: a versioned schema with optional extension namespaces, so the
IR can grow without breaking consumers.
4. **Performance**: a Rust core aimed at extremely fast rendering for typical tables.
5. **Testability**: the same corpus drives textual snapshots for every writer and a
visual regression gate (see [Testing](#testing)).
6. **Language-agnostic**: a stable C ABI with first-class Python (PyO3) and R
(extendr) bindings.

## Output formats

Gridwell renders the same IR to eleven targets spanning the web, typesetting,
word-processing, and the terminal, so one table description can serve
documentation, print, and office workflows alike. They fall into two groups by the
kind of artifact produced: **text** formats return a string you can inline or write
to a file, while the **packaged** OOXML formats return bytes — a ZIP of XML parts.
(OOXML content is itself XML text; it's the zipped `.docx`/`.xlsx`/`.pptx` container
that makes the output binary.)

| | Formats |
|---|---|
| **Text** | HTML · LaTeX · Typst · RTF · SVG · ANSI (terminal) · Pandoc AST · Quarto |
| **Packaged (OOXML)** | Word `.docx` · Excel `.xlsx` · PowerPoint `.pptx` |

Each format is an independent `gridwell-writer-*` crate, so consumers can choose to
depend only on what they need.

## The IR

The IR is a declarative, versioned, JSON description of a fully-resolved table. Here
are its defining characteristics:

- **Rendering-oriented, not computational.** It carries already-decided presentation
(formatted values, resolved styles), not formulas or data sources.
- **A materialized grid.** Every row has exactly `config.table_cols` cells, and a
cell's index *is* its starting column. Spans are explicit: a `colspan(N)` cell is
followed by `N−1` placeholder cells; `rowspan` places placeholders at the same index
in later rows. A validator materializes the grid to catch overlaps, gaps, and
overflows before rendering.
- **Versioned and open at the edges.** Has an `ir_version` field, an `extensions`
blob, and an open content-node enum so unknown node types round-trip losslessly.

Here is the top-level shape of the IR:

```jsonc
{
  "ir_version": "1.0",
  "config":      { "table_cols": 3, "stub_cols": 1, "row_striping": false, ... },
  "styles":      { "defs": {}, "compositions": {}, "conditionals": [] },
  "header":      { "title": ..., "subtitle": ... },
  "column_spec": [ { "id": "col_0", "align": "left", "width": "auto", ... } ],
  "table":       { "thead": { "rows": [...] }, "tbody": [ /* row groups */ ] },
  "footer":      { "footnotes": [...], "source_notes": [...] },
  "extensions":  {}
}
```

Cells carry rich `content` (text, styled text, line breaks, footnote marks, images,
raw per-format markup) plus an optional `typed_value` (which is the raw value + type)
that sits alongside the display text (see the design contract).

The full schema is in the [documentation site](https://rich-iannone.github.io/gridwell/schema/).

## Architecture

Gridwell is a hub-and-spoke pipeline: every input is parsed into one validated
in-memory table, which then fans out to independent per-format writers, all exposed
through a common set of language surfaces. Keeping that hub narrow is what decouples
inputs from outputs — a new source or a new output format can be added without
touching the others.

```
        Upstream producers
  Great Tables (Py) · gt (R) · your code
                  |
                  │  emit IR (JSON)
                  ▼
┌──────────────────────────────────────────────┐
│                  gridwell (Rust)             │
│                                              │
│   parse ──▶     Table      ──▶  validate     │
│  (gridwell-ir)      │                        │
│                     ▼                        │
│     ┌───────────────────────────────────┐    │
│     │        one writer per format      │    │
│     │  html latex typst rtf svg ansi    │    │
│     │  pandoc quarto docx xlsx pptx     │    │
│     └───────────────────────────────────┘    │
│                     │                        │
│        C ABI · PyO3 · extendr · CLI          │
└──────────────────────────────────────────────┘
                  │
                  ▼
      HTML / PDF / DOCX / XLSX / … output
```

Here, **Table** is the in-memory, validated `gridwell_ir::Table`. It's the single
representation that every input funnels into and every writer reads from (not a
separate artifact as it's the same schema as the IR JSON, just parsed and checked).

The workspace is a set of small, focused crates:

```
crates/
  gridwell-ir/           IR types, serde (de)serialization, and the validator
  gridwell-core/          shared primitives (length units, colors)
  gridwell-writer-html/    ─┐
  gridwell-writer-latex/    │
  gridwell-writer-typst/    │
  gridwell-writer-rtf/      │  one crate per output format
  gridwell-writer-svg/      ├─ (IR → format string, or → bytes for OOXML)
  gridwell-writer-ansi/     │
  gridwell-writer-pandoc/   │
  gridwell-writer-quarto/   │
  gridwell-writer-docx/     │
  gridwell-writer-xlsx/     │
  gridwell-writer-pptx/    ─┘
  gridwell-ffi/           C ABI surface (cdylib / staticlib)
  gridwell-python/        Python bindings (PyO3, built with maturin)
  gridwell-r/             R bindings (extendr)
  gridwell-cli/           `gridwell` command-line tool
  gridwell-testkit/       shared example corpus + a builder DSL for the IR
xtask/                    visual test harness (render gallery + image diff)
docs/                     Quarto documentation site
harness/                  pinned renderer toolchain + goldens for visual tests
```

### Design contract: who does what

Gridwell draws a firm line between *deciding* and *rendering*:

- **Upstream resolves** the data, number/date formatting, locale, and which styles
apply to which cells, then, the fully-baked IR is emitted.
- **Gridwell renders** that IR faithfully to each target and it does not re-run
formatting logic or interpret data.

The one deliberate extra is a cell's `typed_value`: the raw typed value (e.g. a
number, boolean, or date) that travels alongside the human-readable `content`, so
value-aware targets (Excel cells, sortable HTML) can use the real value while every
other target uses the pre-formatted text.

> A secondary, planned mode lifts existing **HTML+CSS** (the kind that Great Tables/gt
> already produce) back into the IR, enabling format conversion without upstream
> changes.

## Language bindings

| Surface | How |
|---|---|
| **Rust** | `gridwell_ir::Table::{from_json, to_json, validate}` + the `gridwell-writer-*` crates |
| **CLI** | `gridwell convert -t <format> [input] [-o out]`, `gridwell validate`, `gridwell formats` |
| **Python** | PyO3 module built with maturin (e.g. `gridwell.parse_ir(...)`) |
| **R** | extendr wrapper |

```bash
# Render an IR JSON file to each format
gridwell convert table.json -t html                 # text -> stdout
gridwell convert table.json -t latex -o table.tex
gridwell convert table.json -t docx  -o table.docx  # binary -> file
gridwell validate table.json                        # structural checks
```

In Rust, the [`gridwell-testkit`](crates/gridwell-testkit/src/builder.rs) builder is
the ergonomic way to construct valid IR without hand-writing JSON.

## Testing

The same example corpus (defined once in `gridwell-testkit`) feeds two layers:

- **Textual snapshots**: every writer renders every example and asserts an `insta`
snapshot; the committed `.snap` files are the accepted baseline, reviewed with
`cargo insta review`.
- **Visual harness**: `cargo xtask gallery` renders each example to each format,
rasterizes to PNGs, and builds a browsable gallery; a perceptual image diff gates a
deterministic subset (HTML/SVG/Typst) against committed goldens produced in a pinned
Docker toolchain.

See [`harness/README.md`](harness/README.md) for the full workflow.

```bash
cargo test                 # unit/validation tests + all writer snapshots
cargo xtask gallery        # build the local render gallery (harness/gallery/index.html)
```

## Status & docs

Gridwell is under active development. User-facing documentation (the guide, the
full IR schema, the API reference, and rendered demos) is published at
<https://rich-iannone.github.io/gridwell/>, built from the Quarto sources in
[`docs/`](docs/) via CI.

## License

MIT. See [`LICENSE`](LICENSE).
