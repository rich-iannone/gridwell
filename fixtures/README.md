# Gridwell Fixture Corpus

Canonical test fixtures for the Gridwell table IR. These serve as both test cases and
documentation of expected behavior — similar to how commonmark's `spec.txt` defines
Markdown-to-HTML conversion expectations.

## Directory Structure

```
gridwell_fixtures/
├── comprehensive/         Tables exercising many features together
├── minimal/               One feature per fixture, smallest possible table
└── invalid/               Must fail validation with a specific error
```

## Fixture Format

Every fixture is a JSON file conforming to the Gridwell IR schema (`ir_version: "1.0"`).

- **Valid fixtures** (`comprehensive/`, `minimal/`) must pass validation with zero errors.
- **Invalid fixtures** (`invalid/`) include a `_comment` field documenting the expected
  validation error.

## Comprehensive Fixtures

| File | Description | Features Exercised |
|------|-------------|-------------------|
| `reference_table.json` | Regional sales table | Title, subtitle, spanner (colspan=3), rowspan=2, row groups, summary rows, footnote, source note, stub, typed values, 13 styles |

## Minimal Fixtures

Each tests one specific IR capability in isolation.

| File | Tests | Shape |
|------|-------|-------|
| `minimal_1x1.json` | Absolute minimum valid IR | 1×1 |
| `minimal_no_header.json` | Table with no title/subtitle/footer | 2×2 |
| `colspan_basic.json` | Single colspan=2 spanner in thead | 3×2 (2 header rows) |
| `rowspan_basic.json` | Single rowspan=2 in tbody with placeholder | 2×3 |
| `footnote_single.json` | One mark in a cell, one definition in footer | 2×2 |
| `row_group_multiple.json` | Two row groups with labels, no summaries | 3×4 |
| `summary_rows.json` | Row group with a summary row | 3×2 + summary |
| `empty_body.json` | Zero body rows, header only | 3×0 |
| `styles_borders.json` | Solid, dashed, double borders + row striping | 3×3 |
| `content_rich.json` | styled_text, line_break, image, footnote_mark | 2×2 |
| `unicode_cjk.json` | CJK characters (Japanese) with title | 2×2 |
| `column_widths.json` | Mix of px, %, fr, auto widths | 5×2 |

## Invalid Fixtures

Each must trigger a specific validation error.

| File | Expected Error |
|------|---------------|
| `span_overflow_right.json` | `SPAN_OVERFLOW_RIGHT` — colspan extends past right edge |
| `span_overflow_bottom.json` | `SPAN_OVERFLOW_BOTTOM` — rowspan extends past bottom of group |
| `span_overlap.json` | `SPAN_OVERLAP` — two spans claim same grid position |
| `footnote_ref_missing.json` | `FOOTNOTE_REFS_VALID` — mark references undefined footnote |
| `style_ref_missing.json` | `STYLE_REFS_VALID` — cell references undefined style |
| `col_count_mismatch.json` | `COL_COUNT` — row has more cells than config.table_cols |
| `placeholder_has_content.json` | `SPAN_PLACEHOLDER_HAS_CONTENT` — placeholder cell not empty |
| `summary_no_stub.json` | `SUMMARY_REQUIRES_STUB` — summary row with stub_cols=0 |

## Adding New Fixtures

1. Choose the appropriate tier:
   - **comprehensive/** — multi-feature, realistic table
   - **minimal/** — isolates exactly one feature
   - **invalid/** — must trigger exactly one validation error

2. Name the file descriptively (`feature_variant.json`).

3. For invalid fixtures, include a `_comment` field at the top level:
   ```json
   { "_comment": "INVALID: description. Expects RULE_NAME." }
   ```

4. Validate against the IR schema before committing (once `gridwell-ir` is implemented).

5. Update this README with the new fixture in the appropriate table.

## Relationship to Tests

In the Rust codebase, fixtures are loaded by test functions:

```rust
fn load_fixture(name: &str) -> Table {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../gridwell_fixtures")
        .join(name);
    let json = std::fs::read_to_string(path).unwrap();
    Table::from_json(&json).unwrap()
}

#[test]
fn test_minimal_1x1_validates() {
    let table = load_fixture("minimal/minimal_1x1.json");
    let errors = validate(&table);
    assert!(errors.is_empty());
}

#[test]
fn test_span_overflow_right_detected() {
    let table = load_fixture("invalid/span_overflow_right.json");
    let errors = validate(&table);
    assert!(errors.iter().any(|e| e.rule == Rule::SpanOverflowRight));
}
```

Every valid fixture becomes a positive test case. Every invalid fixture becomes a negative
test case. Writers use valid fixtures for snapshot testing.
