# Gridwell IR Schema Reference

Version: 1.0

## Design Principles

1. **Declarative** — describes *what* to render, not *how*
2. **Flat where possible** — uses indices/references, avoids deep nesting
3. **Typed values** — cells carry typed data alongside formatted text
4. **Styling by reference** — styles defined in a palette, referenced by ID
5. **Span-aware** — row/column spans are first-class with explicit placeholders
6. **Versioned** — `ir_version` allows future evolution

## Top-Level Fields

| Field         | Type     | Required | Description                            |
|---------------|----------|----------|----------------------------------------|
| `ir_version`  | string   | Yes      | Schema version (currently `"1.0"`)     |
| `config`      | object   | Yes      | Table-level configuration              |
| `styles`      | object   | Yes      | Style palette                          |
| `header`      | object   | No       | Title, subtitle, extra header content  |
| `column_spec` | array    | Yes      | Column definitions                     |
| `table`       | object   | Yes      | Table data (thead + tbody)             |
| `footer`      | object   | No       | Footnotes and source notes             |
| `extensions`  | object   | No       | Arbitrary extension data               |

## Config

| Field                        | Type    | Default   | Description                      |
|------------------------------|---------|-----------|----------------------------------|
| `table_cols`                 | integer | —         | Number of columns                |
| `header_rows`                | integer | —         | Number of header rows            |
| `body_rows`                  | integer | —         | Number of body rows              |
| `stub_cols`                  | integer | 0         | Number of stub (row label) columns |
| `row_striping`               | boolean | false     | Enable alternating row colors    |
| `row_striping_include_stub`  | boolean | false     | Include stub in striping         |
| `row_striping_include_body`  | boolean | false     | Include body in striping         |
| `column_labels_hidden`       | boolean | false     | Hide column header row           |
| `table_width`                | string? | null      | CSS-like width value             |
| `container_width`            | string? | null      | Container width                  |
| `container_height`           | string? | null      | Container height                 |
| `container_overflow`         | string? | null      | Overflow behavior                |
| `locale`                     | string  | "en-US"   | Locale for formatting            |
| `page_break_mode`            | string  | "avoid"   | Page break handling              |
| `aria_label`                 | string? | null      | Accessibility label              |
| `aria_describedby`           | string? | null      | Accessibility description ref    |
| `summary`                    | string? | null      | Table summary text               |

## Column Spec

Each element in `column_spec` defines one column:

| Field       | Type    | Required | Description                          |
|-------------|---------|----------|--------------------------------------|
| `id`        | string  | Yes      | Unique column identifier             |
| `align`     | string  | Yes      | `"left"`, `"center"`, `"right"`, or `"char"` |
| `align_char`| string? | No       | Character for char alignment (e.g. `"."`) |
| `width`     | string  | Yes      | `"auto"` or CSS length (e.g. `"120px"`) |
| `min_width` | string? | No       | Minimum width                        |
| `max_width` | string? | No       | Maximum width                        |
| `style_id`  | string? | No       | Default style for this column        |
| `hidden`    | boolean | Yes      | Whether column is hidden             |
| `label`     | string  | No       | Column display label                 |

## Table Block

```
table: {
  thead: { rows: [Row, ...] },
  tbody: [RowGroup, ...]
}
```

### Row

| Field     | Type    | Description                              |
|-----------|---------|------------------------------------------|
| `role`    | string? | `"column_label"`, `"group_label"`, etc.  |
| `style_id`| string? | Style reference                          |
| `cells`   | array   | Array of Cell objects                    |

### Cell

| Field           | Type    | Required | Description                       |
|-----------------|---------|----------|-----------------------------------|
| `content`       | array   | Yes      | Array of ContentNode              |
| `typed_value`   | object? | No       | Typed data value                  |
| `colspan`       | integer | Yes      | Column span (≥ 1)                 |
| `rowspan`       | integer | Yes      | Row span (≥ 1)                    |
| `style_id`      | string? | No       | Style reference                   |
| `is_stub`       | boolean | Yes      | Whether cell is a stub (row label)|
| `is_placeholder`| boolean | Yes      | Whether cell is a span placeholder|
| `scope`         | string? | No       | `"col"`, `"row"`, `"colgroup"`, `"rowgroup"` |
| `sort_key`      | string? | No       | Sort value for this cell          |
| `data_type`     | string? | No       | Data type hint                    |
| `footnote_refs` | array?  | No       | Array of footnote IDs             |

### ContentNode

| Type         | Fields              |
|--------------|---------------------|
| `text`       | `value`             |
| `code`       | `value`             |
| `emphasis`   | `value`             |
| `strong`     | `value`             |
| `link`       | `value`, `href`     |
| `image`      | `src`, `alt`        |
| `linebreak`  | —                   |
| `superscript`| `value`             |
| `subscript`  | `value`             |
| `markdown`   | `value`             |

### TypedValue

| Field  | Type   | Description                               |
|--------|--------|-------------------------------------------|
| `type` | string | `"string"`, `"number"`, `"integer"`, `"date"`, `"time"`, `"datetime"`, `"boolean"`, `"currency"` |
| `value` | string | The raw value as a string                 |

### RowGroup

| Field          | Type    | Description                        |
|----------------|---------|-------------------------------------|
| `group_id`     | string? | Group identifier                    |
| `label`        | string? | Display label for the group         |
| `rows`         | array   | Array of Row objects                |
| `summary_rows` | array   | Summary/aggregate rows              |

## Styles

```json
"styles": {
  "defs": { "<style_id>": StyleDef, ... },
  "compositions": { "<comp_id>": [style_id, ...], ... },
  "conditionals": [ConditionalRule, ...]
}
```

### StyleDef

All fields are optional:

| Field              | Type    | Description                    |
|--------------------|---------|--------------------------------|
| `font_weight`      | string  | `"normal"`, `"bold"`           |
| `font_style`       | string  | `"normal"`, `"italic"`         |
| `font_size`        | string  | CSS font-size value            |
| `font_family`      | string  | Font family name               |
| `color`            | string  | Text color (hex)               |
| `background_color` | string  | Background color (hex)         |
| `text_align`       | string  | `"left"`, `"center"`, `"right"`|
| `vertical_align`   | string  | `"top"`, `"middle"`, `"bottom"`|
| `text_decoration`  | string  | `"none"`, `"underline"`, etc.  |
| `text_transform`   | string  | `"none"`, `"uppercase"`, etc.  |
| `padding_top`      | string  | CSS padding value              |
| `padding_right`    | string  | CSS padding value              |
| `padding_bottom`   | string  | CSS padding value              |
| `padding_left`     | string  | CSS padding value              |
| `border_top`       | Border  | Top border definition          |
| `border_right`     | Border  | Right border definition        |
| `border_bottom`    | Border  | Bottom border definition       |
| `border_left`      | Border  | Left border definition         |
| `white_space`      | string  | CSS white-space value          |
| `overflow`         | string  | CSS overflow value             |
| `indent`           | string  | Text indent                    |

### Border

| Field   | Type   | Description                      |
|---------|--------|----------------------------------|
| `width` | string | Border width (e.g. `"1px"`)      |
| `style` | string | `"solid"`, `"dashed"`, `"dotted"`, `"double"`, `"none"` |
| `color` | string | Border color (hex)               |

## Footer

| Field          | Type  | Description                     |
|----------------|-------|---------------------------------|
| `footnotes`    | array | Array of Footnote objects        |
| `source_notes` | array | Array of SourceNote objects      |

### Footnote

| Field    | Type   | Description                        |
|----------|--------|------------------------------------|
| `id`     | string | Unique footnote identifier         |
| `content`| array  | Array of ContentNode               |

### SourceNote

| Field    | Type  | Description                         |
|----------|-------|-------------------------------------|
| `content`| array | Array of ContentNode                |

## Validation Rules

The validator checks:

1. **Column count**: Each row must have cells summing to `config.table_cols` (accounting for colspan)
2. **Span bounds**: Spans must not overflow table dimensions
3. **Span overlap**: Spanned regions must not overlap
4. **Placeholder consistency**: Placeholders must have empty content
5. **Style references**: All `style_id` values must exist in `styles.defs`
6. **Footnote references**: All `footnote_refs` must reference existing footnotes
7. **Summary rows**: Only valid when `stub_cols > 0`
