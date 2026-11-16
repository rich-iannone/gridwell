# gridwell-cli

The `gridwell` command: render a table described in gridwell's IR (JSON) to HTML,
LaTeX, Typst, RTF, SVG, the terminal, the Pandoc and Quarto ASTs, and Word, Excel
and PowerPoint files.

```bash
cargo install --locked --git https://github.com/rich-iannone/gridwell gridwell-cli

gridwell convert table.json --to latex
gridwell convert table.json -o table.docx
gridwell validate table.json
gridwell inspect table.json
gridwell formats
```

`gridwell fmt` writes IR in canonical form, `gridwell grid` draws the grid of cells
that spans are checked against, and `gridwell schema` prints the IR's JSON Schema.
Every command is described in the
[documentation](https://rich-iannone.github.io/gridwell/guide/cli.html). The Rust
library is the [`gridwell`](https://rich-iannone.github.io/gridwell/api/rust.html)
crate.

License: MIT.
