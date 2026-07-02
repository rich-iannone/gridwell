#!/usr/bin/env bash
# Regenerates the rendered SVG images for the GT Integration demo page.
# Requirements: typst, pdflatex (with booktabs, multirow), pdf2svg, Python venv with gridwell + great_tables
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

cd "$REPO_ROOT"
source .venv/bin/activate

# Generate Typst source and render to SVG
python -c "
import gridwell
from great_tables import GT, exibble

gt = (
    GT(exibble.head(4), rowname_col='row', groupname_col='group')
    .tab_header(title='Exibble Sample', subtitle='First 4 rows')
    .tab_source_note(source_note='Source: great_tables package')
)
table = gridwell.parse_ir(gridwell.gt_to_ir(gt))
print(table.render_typst())
" > /tmp/gridwell_demo.typ

typst compile /tmp/gridwell_demo.typ "$SCRIPT_DIR/gt-typst-rendered.svg" --format svg
echo "✓ Typst SVG rendered"

# Generate LaTeX source and render to SVG via PDF
python -c "
import gridwell
from great_tables import GT, exibble

gt = (
    GT(exibble.head(4), rowname_col='row', groupname_col='group')
    .tab_header(title='Exibble Sample', subtitle='First 4 rows')
    .tab_source_note(source_note='Source: great_tables package')
)
table = gridwell.parse_ir(gridwell.gt_to_ir(gt))
latex = table.render_latex()

with open('/tmp/gridwell_demo.tex', 'w') as f:
    f.write(r'\documentclass{article}' + '\n')
    f.write(r'\usepackage[paperwidth=20cm,paperheight=10cm,margin=0.5cm]{geometry}' + '\n')
    f.write(r'\usepackage{booktabs}' + '\n')
    f.write(r'\usepackage{multirow}' + '\n')
    f.write(r'\pagestyle{empty}' + '\n')
    f.write(r'\begin{document}' + '\n')
    f.write(latex + '\n')
    f.write(r'\end{document}' + '\n')
"

pdflatex -interaction=nonstopmode -output-directory=/tmp /tmp/gridwell_demo.tex > /dev/null 2>&1
pdf2svg /tmp/gridwell_demo.pdf "$SCRIPT_DIR/gt-latex-rendered.svg"
echo "✓ LaTeX SVG rendered"

# Cleanup
rm -f /tmp/gridwell_demo.{typ,tex,pdf,aux,log}
echo "Done. SVGs updated in $SCRIPT_DIR/"
