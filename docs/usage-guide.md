# Gridwell Usage Guide

## Overview

Gridwell converts a declarative table IR (JSON) into many output formats. The workflow is:

1. **Create IR** — produce a JSON document describing your table structure
2. **Parse** — load the JSON into gridwell
3. **Validate** (optional) — check the IR for structural errors
4. **Render** — produce output in any supported format

## Table IR Structure

A gridwell table IR is a JSON document with this top-level structure:

```json
{
  "ir_version": "1.0",
  "config": { },
  "styles": { },
  "header": { },
  "column_spec": [ ],
  "table": { },
  "footer": { },
  "extensions": { }
}
```

### Minimal Example

The simplest possible table (1 column, 1 header row, 1 body row):

```json
{
  "ir_version": "1.0",
  "config": {
    "table_cols": 1,
    "header_rows": 1,
    "body_rows": 1,
    "stub_cols": 0,
    "row_striping": false,
    "row_striping_include_stub": false,
    "row_striping_include_body": false,
    "column_labels_hidden": false,
    "locale": "en-US",
    "page_break_mode": "avoid"
  },
  "styles": { "defs": {}, "compositions": {}, "conditionals": [] },
  "header": { "title": null, "subtitle": null, "extra_lines": [] },
  "column_spec": [
    { "id": "col_0", "align": "left", "width": "auto", "hidden": false, "label": "Name" }
  ],
  "table": {
    "thead": {
      "rows": [{
        "role": "column_label",
        "cells": [{
          "content": [{ "type": "text", "value": "Name" }],
          "colspan": 1, "rowspan": 1,
          "is_stub": false, "is_placeholder": false, "scope": "col"
        }]
      }]
    },
    "tbody": [{
      "rows": [{
        "cells": [{
          "content": [{ "type": "text", "value": "Alice" }],
          "typed_value": { "type": "string", "value": "Alice" },
          "colspan": 1, "rowspan": 1,
          "is_stub": false, "is_placeholder": false,
          "data_type": "string"
        }]
      }],
      "summary_rows": []
    }]
  },
  "footer": { "footnotes": [], "source_notes": [] }
}
```

## Quick Start

### Python

```python
import json
import gridwell

# Load IR from file
with open("my_table.json") as f:
    ir = f.read()

table = gridwell.Table.from_json(ir)

# Check for errors
errors = table.validate()
if errors:
    print(f"Validation errors: {errors}")

# Render to HTML and write to file
with open("output.html", "w") as f:
    f.write(table.render_html())

# Render to DOCX
with open("output.docx", "wb") as f:
    f.write(table.render_docx())
```

### R

```r
library(gridwell)

# Load IR from file
ir <- paste(readLines("my_table.json", warn = FALSE), collapse = "\n")
tbl <- gw_parse_ir(ir)

# Check for errors
errors <- gw_validate(tbl)
if (length(errors) > 0) message("Errors: ", paste(errors, collapse = "; "))

# Render to HTML
writeLines(gw_render_html(tbl), "output.html")

# Render to DOCX
writeBin(gw_render_docx(tbl), "output.docx")
```

### Rust

```rust
use gridwell_ir::Table;
use gridwell_writer_html::render_html;
use gridwell_writer_docx::render_docx;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let json = std::fs::read_to_string("my_table.json")?;
    let table = Table::from_json(&json)?;

    // Validate
    let errors = table.validate();
    if !errors.is_empty() {
        for e in &errors { eprintln!("{e}"); }
        std::process::exit(1);
    }

    // Render
    let html = render_html(&table)?;
    std::fs::write("output.html", &html)?;

    let docx = render_docx(&table)?;
    std::fs::write("output.docx", &docx)?;

    Ok(())
}
```

## Content Nodes

Cell content is an array of content nodes. Each node has a `type` field:

| Type       | Description                       | Fields                    |
|------------|-----------------------------------|---------------------------|
| `text`     | Plain text                        | `value`                   |
| `code`     | Inline code                       | `value`                   |
| `emphasis` | Italic text                       | `value`                   |
| `strong`   | Bold text                         | `value`                   |
| `link`     | Hyperlink                         | `value`, `href`           |
| `image`    | Inline image                      | `src`, `alt`              |
| `linebreak`| Line break within a cell          | —                         |
| `superscript` | Superscript text               | `value`                   |
| `subscript`| Subscript text                    | `value`                   |
| `markdown` | Raw markdown (writer interprets)  | `value`                   |

Example:

```json
"content": [
  { "type": "strong", "value": "Total" },
  { "type": "text", "value": ": " },
  { "type": "code", "value": "$1,234" }
]
```

## Typed Values

Body cells can carry a `typed_value` for numeric/date-aware rendering:

```json
"typed_value": { "type": "number", "value": "1234.56" }
```

Supported types: `string`, `number`, `integer`, `date`, `time`, `datetime`, `boolean`, `currency`.

## Column Spans and Row Spans

Cells support `colspan` and `rowspan` > 1. Spanned-over positions must have placeholder cells:

```json
{
  "content": [],
  "colspan": 1, "rowspan": 1,
  "is_placeholder": true,
  "is_stub": false
}
```

## Styles

Styles are defined in the `styles.defs` palette and referenced by ID:

```json
"styles": {
  "defs": {
    "header_bold": {
      "font_weight": "bold",
      "background_color": "#f0f0f0",
      "border_bottom": { "width": "2px", "style": "solid", "color": "#333" }
    }
  }
}
```

Then referenced in cells or rows:

```json
{ "content": [...], "style_id": "header_bold", ... }
```

## Row Groups

The table body is an array of row groups. Each group can have:
- `group_id` — identifier for the group
- `label` — display label (rendered as a group header row)
- `rows` — the data rows
- `summary_rows` — aggregate rows shown after the group

## Footer

The footer contains footnotes and source notes:

```json
"footer": {
  "footnotes": [
    { "id": "fn1", "content": [{ "type": "text", "value": "p < 0.05" }] }
  ],
  "source_notes": [
    { "content": [{ "type": "text", "value": "Data from WHO, 2024." }] }
  ]
}
```

Footnotes are referenced from cells via `footnote_refs`:

```json
{ "content": [...], "footnote_refs": ["fn1"], ... }
```

## Format-Specific Notes

### HTML
- Produces semantic HTML5 with `<table>`, `<thead>`, `<tbody>`, `<tfoot>`
- Styles rendered as a `<style>` block or inline (configurable)

### LaTeX
- Produces `\begin{longtable}` with proper column alignment
- Supports `\multicolumn` and `\multirow` for spans

### Typst
- Produces `#table(...)` markup
- Column widths mapped to Typst length units

### RTF
- Full RTF 1.9 document with color table and font table
- Supports cell merging via `\clmgf`/`\clmrg`

### SVG
- Standalone SVG with embedded text layout
- Configurable font and dimensions

### Pandoc
- Pandoc AST JSON suitable for `pandoc -f json`
- Supports all inline formatting types

### Quarto
- Pipe table Markdown with Quarto div wrappers
- Metadata preserved as YAML attributes

### DOCX / XLSX / PPTX
- Valid Office Open XML archives (ZIP)
- Can be opened directly in Microsoft Office or LibreOffice
