"""
Reference Great Tables table for gridwell IR development.

This produces a regional sales table with:
- Title + subtitle (header)
- Spanner labels (colspan in thead)
- Column labels
- Stub column (region/country)
- Two row groups with labels
- One rowspan (merged country cells)
- Summary rows per group
- A footnote on one cell
- Source note in footer
- Mixed styling (borders, colors, alignment)
"""

import polars as pl
from great_tables import GT, loc, style

df = pl.DataFrame(
    {
        "country": [
            "United States",
            "United States",
            "Canada",
            "Germany",
            "Germany",
            "France",
        ],
        "year": [2023, 2024, 2024, 2023, 2024, 2024],
        "revenue": [1250.3, 1380.7, 420.1, 890.5, 945.2, 610.8],
        "expenses": [980.2, 1050.4, 310.6, 720.1, 780.3, 490.2],
        "profit": [270.1, 330.3, 109.5, 170.4, 164.9, 120.6],
        "margin_pct": [21.6, 23.9, 26.1, 19.1, 17.4, 19.7],
    }
)

gt_table = (
    GT(df, rowname_col="country", groupname_col=None)
    .tab_header(
        title="Regional Sales Performance",
        subtitle="Fiscal years 2023–2024 (in millions USD)",
    )
    .tab_spanner(label="Financials", columns=["revenue", "expenses", "profit"])
    .tab_spanner(label="KPI", columns=["margin_pct"])
    .cols_label(
        year="Year",
        revenue="Revenue",
        expenses="Expenses",
        profit="Profit",
        margin_pct="Margin %",
    )
    .tab_row_group(label="North America", rows=[0, 1, 2])
    .tab_row_group(label="Europe", rows=[3, 4, 5])
    .tab_source_note(source_note="Source: Internal finance database, June 2025.")
    .tab_footnote(
        footnote="Includes one-time restructuring charge.",
        locations=loc.body(columns="expenses", rows=[1]),
    )
    .fmt_number(columns=["revenue", "expenses", "profit"], decimals=1, use_seps=True)
    .fmt_number(columns="margin_pct", decimals=1, pattern="{x}%")
    .tab_style(
        style=style.text(weight="bold"),
        locations=loc.body(columns="profit"),
    )
    .tab_style(
        style=style.borders(sides="bottom", weight="2px", color="#000000"),
        locations=loc.column_labels(),
    )
)

if __name__ == "__main__":
    print(gt_table.as_raw_html())
