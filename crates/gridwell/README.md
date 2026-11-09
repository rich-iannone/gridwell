# gridwell

Render one table, described once in gridwell's IR (JSON), to HTML, LaTeX, Typst,
RTF, SVG, the terminal, the Pandoc and Quarto ASTs, and Word, Excel and PowerPoint
files.

```rust
let json = std::fs::read_to_string("table.json")?;
let table = gridwell::Table::from_json(&json)?;

// By format name, with the format's options as JSON
let html = gridwell::render(&table, "html", None)?;

// Straight to a file, the format from the extension
gridwell::save(&table, std::path::Path::new("table.docx"), None, None)?;
```

Every format is a cargo feature named as the format is (`html`, `latex`, `typst`,
`rtf`, `svg`, `ansi`, `pandoc`, `quarto`, `docx`, `xlsx`, `pptx`), all on by
default. To build only the formats you need:

```toml
[dependencies]
gridwell = { version = "0.1", default-features = false, features = ["html", "latex"] }
```

`svg`, `ansi`, `pandoc` and `quarto` are experimental for now: their output
is valid and has every cell, but their styling is best-effort and may change between
releases (`Writer::experimental()` says which formats are).

The IR, the CLI (`cargo install gridwell-cli`) and the Python and R packages are
described in the [documentation](https://rich-iannone.github.io/gridwell/).

License: MIT.
