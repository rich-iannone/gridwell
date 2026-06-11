# Gridwell API Reference

Gridwell is a Rust library for rendering table IR (Intermediate Representation) to many output formats. It provides bindings for Rust, Python, R, and C.

## Supported Output Formats

| Format | Type   | Function/Method             |
|--------|--------|-----------------------------|
| HTML   | Text   | `render_html`               |
| LaTeX  | Text   | `render_latex`              |
| Typst  | Text   | `render_typst`              |
| RTF    | Text   | `render_rtf`                |
| SVG    | Text   | `render_svg`                |
| ANSI   | Text   | `render_ansi`               |
| Pandoc | Text   | `render_pandoc`             |
| Quarto | Text   | `render_quarto`             |
| DOCX   | Binary | `render_docx`               |
| XLSX   | Binary | `render_xlsx`               |
| PPTX   | Binary | `render_pptx`               |

---

## Rust API

### Parsing

```rust
use gridwell_ir::Table;

let json = std::fs::read_to_string("table.json")?;
let table = Table::from_json(&json)?;
```

### Validation

```rust
let errors = table.validate();
if errors.is_empty() {
    println!("Table is valid");
} else {
    for err in &errors {
        eprintln!("Validation error: {err}");
    }
}
```

### Text Rendering

Each text writer provides a convenience function and a configurable writer struct:

```rust
use gridwell_writer_html::{render_html, HtmlWriter, HtmlWriterConfig};

// Default settings
let html = render_html(&table)?;

// Custom config
let config = HtmlWriterConfig {
    inline_styles: true,
    pretty_print: false,
    class_prefix: "my-table".to_string(),
};
let writer = HtmlWriter::with_config(config);
let html = writer.render(&table)?;
```

Available convenience functions:

```rust
gridwell_writer_html::render_html(&table)?;
gridwell_writer_latex::render_latex(&table)?;
gridwell_writer_typst::render_typst(&table)?;
gridwell_writer_rtf::render_rtf(&table)?;
gridwell_writer_svg::render_svg(&table)?;
gridwell_writer_ansi::render_ansi(&table)?;
gridwell_writer_pandoc::render_pandoc(&table)?;
gridwell_writer_quarto::render_quarto(&table)?;
```

### Binary Rendering

Binary writers produce `Vec<u8>` containing the file bytes:

```rust
use gridwell_writer_docx::render_docx;
use gridwell_writer_xlsx::render_xlsx;
use gridwell_writer_pptx::render_pptx;

let docx_bytes = render_docx(&table)?;
std::fs::write("output.docx", &docx_bytes)?;

let xlsx_bytes = render_xlsx(&table)?;
std::fs::write("output.xlsx", &xlsx_bytes)?;

let pptx_bytes = render_pptx(&table)?;
std::fs::write("output.pptx", &pptx_bytes)?;
```

### Serialization

```rust
let json_output = table.to_json()?;
```

---

## Python API

### Installation

```bash
pip install gridwell
```

### Usage

```python
import gridwell

# Parse from JSON string
table = gridwell.Table.from_json(json_str)

# Parse from dict
table = gridwell.Table.from_dict({"ir_version": "1.0", ...})

# Or use the shorthand
table = gridwell.parse_ir(json_str)

# Validate
errors = table.validate()  # returns list of strings

# Render to text formats
html = table.render_html()
latex = table.render_latex()
typst = table.render_typst()
rtf = table.render_rtf()
svg = table.render_svg()
ansi = table.render_ansi()
pandoc = table.render_pandoc()
quarto = table.render_quarto()

# Generic text render by name
html = table.render("html")

# Render to binary formats (returns bytes)
docx_bytes = table.render_docx()
xlsx_bytes = table.render_xlsx()
pptx_bytes = table.render_pptx()

# Generic binary render by name
docx_bytes = table.render_binary("docx")

# Write binary output to file
with open("output.docx", "wb") as f:
    f.write(table.render_docx())

# Serialize back to JSON
json_str = table.to_json()

# Repr
repr(table)  # "Table(ir_version='1.0')"
```

---

## R API

### Installation

```r
# From source (requires Rust toolchain):
install.packages("gridwell", repos = NULL, type = "source")
```

### Usage

```r
library(gridwell)

# Parse from JSON string
json <- readLines("table.json", warn = FALSE) |> paste(collapse = "\n")
tbl <- gw_parse_ir(json)

# Validate
errors <- gw_validate(tbl)  # character vector

# Render to text formats
html  <- gw_render_html(tbl)
latex <- gw_render_latex(tbl)
typst <- gw_render_typst(tbl)
rtf   <- gw_render_rtf(tbl)
svg   <- gw_render_svg(tbl)
ansi  <- gw_render_ansi(tbl)
pandoc <- gw_render_pandoc(tbl)
quarto <- gw_render_quarto(tbl)

# Generic text render by name
html <- gw_render(tbl, "html")

# Render to binary formats (returns raw vector)
docx_raw <- gw_render_docx(tbl)
xlsx_raw <- gw_render_xlsx(tbl)
pptx_raw <- gw_render_pptx(tbl)

# Generic binary render by name
docx_raw <- gw_render_binary(tbl, "docx")

# Write binary output to file
writeBin(gw_render_docx(tbl), "output.docx")

# Serialize back to JSON
json_out <- gw_to_json(tbl)
```

---

## C FFI API

### Header

```c
#include <stdint.h>
#include <stddef.h>

typedef struct GridwellTable GridwellTable;
typedef struct GridwellError GridwellError;

typedef struct {
    char *text;
    size_t len;
} GridwellTextResult;

typedef struct {
    uint8_t *data;
    size_t len;
} GridwellBinaryResult;

// Error codes
#define GRIDWELL_ERR_PARSE       1
#define GRIDWELL_ERR_VALIDATE    2
#define GRIDWELL_ERR_RENDER      3
#define GRIDWELL_ERR_INVALID_ARG 4

// Parse JSON into a table
GridwellTable *gridwell_parse_ir(
    const char *json, size_t json_len, GridwellError **err);

// Validate a table
GridwellError *gridwell_validate(const GridwellTable *table);

// Render to text format
GridwellTextResult gridwell_render_text(
    const GridwellTable *table, const char *format, GridwellError **err);

// Render to binary format
GridwellBinaryResult gridwell_render_binary(
    const GridwellTable *table, const char *format, GridwellError **err);

// Error inspection
const char *gridwell_error_message(const GridwellError *err);
uint32_t gridwell_error_code(const GridwellError *err);

// Memory management
void gridwell_free_table(GridwellTable *table);
void gridwell_free_error(GridwellError *err);
void gridwell_free_text_result(GridwellTextResult result);
void gridwell_free_binary_result(GridwellBinaryResult result);
```

### Example (C)

```c
#include "gridwell.h"
#include <stdio.h>
#include <stdlib.h>

int main(void) {
    const char *json = "{ ... }";  // your table IR JSON
    size_t json_len = strlen(json);
    GridwellError *err = NULL;

    // Parse
    GridwellTable *table = gridwell_parse_ir(json, json_len, &err);
    if (!table) {
        fprintf(stderr, "Parse error: %s\n", gridwell_error_message(err));
        gridwell_free_error(err);
        return 1;
    }

    // Validate
    GridwellError *val_err = gridwell_validate(table);
    if (val_err) {
        fprintf(stderr, "Validation: %s\n", gridwell_error_message(val_err));
        gridwell_free_error(val_err);
    }

    // Render to HTML
    GridwellTextResult result = gridwell_render_text(table, "html", &err);
    if (result.text) {
        printf("HTML (%zu bytes):\n%.*s\n", result.len, (int)result.len, result.text);
        gridwell_free_text_result(result);
    } else {
        fprintf(stderr, "Render error: %s\n", gridwell_error_message(err));
        gridwell_free_error(err);
    }

    // Render to DOCX
    GridwellBinaryResult bin = gridwell_render_binary(table, "docx", &err);
    if (bin.data) {
        FILE *f = fopen("output.docx", "wb");
        fwrite(bin.data, 1, bin.len, f);
        fclose(f);
        gridwell_free_binary_result(bin);
    }

    gridwell_free_table(table);
    return 0;
}
```

### Linking

```bash
# Static linking
cc -o myapp myapp.c -L/path/to/gridwell/target/release -lgridwell_ffi \
   -framework Security -framework CoreFoundation

# Dynamic linking
cc -o myapp myapp.c -L/path/to/gridwell/target/release -lgridwell_ffi
```

---

## Error Handling

All APIs follow a consistent error model:

- **Rust**: Returns `Result<T, E>` where `E` is a format-specific error type.
- **Python**: Raises `ValueError` with a descriptive message.
- **R**: Raises a condition (error) with a descriptive message.
- **C FFI**: Returns NULL/empty result and writes to an `GridwellError**` out-parameter.

### Error Categories

| Code | Name           | Meaning                              |
|------|----------------|--------------------------------------|
| 1    | `ERR_PARSE`    | JSON parsing failed                  |
| 2    | `ERR_VALIDATE` | Table IR validation failed           |
| 3    | `ERR_RENDER`   | Rendering to the requested format failed |
| 4    | `ERR_INVALID_ARG` | NULL pointer or invalid argument  |
