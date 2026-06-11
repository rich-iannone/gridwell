"""Convert a Great Tables GT object to gridwell IR JSON."""

from __future__ import annotations

import json
from typing import Any

import pandas as pd


def gt_to_ir(gt_obj: Any) -> str:
    """Convert a Great Tables GT object to gridwell IR JSON.

    Parameters
    ----------
    gt_obj : great_tables.GT
        A Great Tables GT object.

    Returns
    -------
    str
        JSON string in gridwell IR format.
    """
    return json.dumps(_gt_to_dict(gt_obj), indent=2)


def gt_to_dict(gt_obj: Any) -> dict:
    """Convert a Great Tables GT object to gridwell IR dict.

    Parameters
    ----------
    gt_obj : great_tables.GT
        A Great Tables GT object.

    Returns
    -------
    dict
        Dictionary in gridwell IR format.
    """
    return _gt_to_dict(gt_obj)


def _gt_to_dict(gt_obj: Any) -> dict:
    """Internal: build the IR dictionary from a GT object."""
    from great_tables._utils_render_html import cast_frame_to_string, replace_null_frame

    # Build the GT to resolve formats
    built = gt_obj._build_data("html")._render_formats("html")

    # Resolve cell text: formatted values override raw data
    str_orig = cast_frame_to_string(built._tbl_data)
    tbl_data = replace_null_frame(built._body.body, str_orig)

    # Column info
    visible_cols = built._boxhead._get_default_columns()
    stub_col_info = built._boxhead._get_stub_column()
    has_stub = stub_col_info is not None

    # All display columns (stub first if present, then data columns)
    all_cols = []
    if has_stub:
        all_cols.append(stub_col_info)
    all_cols.extend(visible_cols)

    n_cols = len(all_cols)
    stub_cols = 1 if has_stub else 0

    # Row grouping
    group_rows = built._stub.group_rows if hasattr(built._stub, "group_rows") else None
    has_groups = group_rows is not None and len(group_rows) > 0

    # Build config
    n_body_rows = len(tbl_data)
    config = _build_config(n_cols, n_body_rows, stub_cols, built)

    # Build column_spec
    column_spec = _build_column_spec(all_cols, has_stub)

    # Build header
    header = _build_header(built._heading)

    # Build thead
    thead = _build_thead(all_cols)

    # Build tbody (with row groups)
    tbody = _build_tbody(tbl_data, all_cols, built, has_stub, has_groups, group_rows)

    # Build footer
    footer = _build_footer(built._source_notes, built._footnotes)

    # Build styles
    styles = _build_styles(built._styles)

    return {
        "ir_version": "1.0",
        "config": config,
        "styles": styles,
        "header": header,
        "column_spec": column_spec,
        "table": {
            "thead": thead,
            "tbody": tbody,
        },
        "footer": footer,
        "extensions": {},
    }


def _build_config(n_cols: int, n_body_rows: int, stub_cols: int, built: Any) -> dict:
    """Build the config section."""
    return {
        "table_cols": n_cols,
        "header_rows": 1,
        "body_rows": n_body_rows,
        "stub_cols": stub_cols,
        "row_striping": False,
        "row_striping_include_stub": False,
        "row_striping_include_body": False,
        "column_labels_hidden": False,
        "table_width": None,
        "container_width": None,
        "container_height": None,
        "container_overflow": None,
        "locale": "en-US",
        "page_break_mode": "avoid",
        "aria_label": None,
        "aria_describedby": None,
        "summary": None,
    }


def _build_column_spec(all_cols: list, has_stub: bool) -> list[dict]:
    """Build column_spec array from ColInfo objects."""
    specs = []
    for i, col in enumerate(all_cols):
        is_stub_col = has_stub and i == 0
        specs.append({
            "id": col.var,
            "align": col.column_align or "left",
            "align_char": None,
            "width": col.column_width or "auto",
            "min_width": None,
            "max_width": None,
            "style_id": None,
            "hidden": False,
            "label": str(col.column_label) if col.column_label else col.var,
        })
    return specs


def _build_header(heading: Any) -> dict:
    """Build the header section from GT Heading."""
    title = None
    subtitle = None

    if heading.title:
        title = {"content": [{"type": "text", "value": str(heading.title)}]}
    if heading.subtitle:
        subtitle = {"content": [{"type": "text", "value": str(heading.subtitle)}]}

    return {
        "title": title,
        "subtitle": subtitle,
        "extra_lines": [],
        "preheader_content": None,
    }


def _build_thead(all_cols: list) -> dict:
    """Build the thead section with column labels."""
    cells = []
    for col in all_cols:
        label = str(col.column_label) if col.column_label else col.var
        cells.append({
            "content": [{"type": "text", "value": label}],
            "colspan": 1,
            "rowspan": 1,
            "style_id": None,
            "is_stub": False,
            "is_placeholder": False,
            "scope": "col",
            "sort_key": None,
            "data_type": None,
        })

    return {
        "rows": [{
            "role": "column_label",
            "style_id": None,
            "cells": cells,
        }],
    }


def _build_tbody(
    tbl_data: pd.DataFrame,
    all_cols: list,
    built: Any,
    has_stub: bool,
    has_groups: bool,
    group_rows: Any,
) -> list[dict]:
    """Build the tbody section with row groups."""
    if has_groups and group_rows:
        groups = []
        for grp in group_rows:
            rows = []
            for idx in grp.indices:
                row = _build_row(tbl_data, int(idx), all_cols, built, has_stub)
                rows.append(row)

            label = None
            if grp.group_label:
                label = {
                    "content": [{"type": "text", "value": str(grp.group_label)}],
                    "style_id": None,
                    "colspan": None,
                }

            groups.append({
                "group_id": grp.group_id,
                "label": label,
                "rows": rows,
                "summary_rows": [],
            })
        return groups
    else:
        # Single group with all rows
        rows = []
        for idx in range(len(tbl_data)):
            row = _build_row(tbl_data, idx, all_cols, built, has_stub)
            rows.append(row)
        return [{
            "group_id": None,
            "label": None,
            "rows": rows,
            "summary_rows": [],
        }]


def _build_row(
    tbl_data: pd.DataFrame,
    row_idx: int,
    all_cols: list,
    built: Any,
    has_stub: bool,
) -> dict:
    """Build a single row."""
    cells = []
    for i, col in enumerate(all_cols):
        is_stub = has_stub and i == 0
        value = tbl_data[col.var].iloc[row_idx]
        text = str(value) if not pd.isna(value) else ""

        # Determine data type from the original data
        orig_val = built._tbl_data[col.var].iloc[row_idx]
        data_type = _infer_data_type(orig_val, col.var, built._tbl_data.dtypes)

        cell = {
            "content": [{"type": "text", "value": text}],
            "colspan": 1,
            "rowspan": 1,
            "style_id": None,
            "is_stub": is_stub,
            "is_placeholder": False,
            "data_type": data_type,
            "sort_key": text.lower() if text else None,
        }

        # Add typed_value for non-stub cells
        if not is_stub and text:
            cell["typed_value"] = {
                "type": data_type or "string",
                "value": str(orig_val) if not pd.isna(orig_val) else text,
            }

        cells.append(cell)

    return {
        "role": None,
        "style_id": None,
        "cells": cells,
    }


def _infer_data_type(value: Any, col_name: str, dtypes: Any) -> str | None:
    """Infer the gridwell data_type from a pandas value/dtype."""
    if pd.isna(value):
        return None

    dtype = dtypes.get(col_name)
    if dtype is not None:
        dtype_str = str(dtype)
        if "float" in dtype_str:
            return "number"
        elif "int" in dtype_str:
            return "integer"
        elif "datetime" in dtype_str:
            return "datetime"
        elif "bool" in dtype_str:
            return "boolean"

    # Heuristic from value
    if isinstance(value, (int,)):
        return "integer"
    elif isinstance(value, (float,)):
        return "number"
    elif isinstance(value, bool):
        return "boolean"

    return "string"


def _build_footer(source_notes: list, footnotes: list) -> dict:
    """Build the footer section."""
    fn_list = []
    for i, fn in enumerate(footnotes):
        fn_text = str(fn) if not isinstance(fn, dict) else fn.get("text", str(fn))
        fn_list.append({
            "id": f"fn{i + 1}",
            "content": [{"type": "text", "value": fn_text}],
        })

    sn_list = []
    for sn in source_notes:
        sn_text = str(sn) if not isinstance(sn, dict) else sn.get("text", str(sn))
        sn_list.append({
            "content": [{"type": "text", "value": sn_text}],
        })

    return {
        "footnotes": fn_list,
        "source_notes": sn_list,
    }


def _build_styles(styles: list) -> dict:
    """Build the styles palette from GT StyleInfo entries."""
    defs: dict[str, dict] = {}

    for i, style_info in enumerate(styles):
        style_id = f"style_{i}"
        style_def: dict[str, Any] = {}

        # Extract style properties from the StyleInfo
        if hasattr(style_info, "styles"):
            for s in style_info.styles:
                if hasattr(s, "color") and s.color:
                    style_def["color"] = s.color
                if hasattr(s, "bgcolor") and s.bgcolor:
                    style_def["background_color"] = s.bgcolor
                if hasattr(s, "weight") and s.weight:
                    style_def["font_weight"] = s.weight
                if hasattr(s, "style") and s.style:
                    style_def["font_style"] = s.style
                if hasattr(s, "size") and s.size:
                    style_def["font_size"] = s.size
                if hasattr(s, "align") and s.align:
                    style_def["text_align"] = s.align

        if style_def:
            defs[style_id] = style_def

    return {
        "defs": defs,
        "compositions": {},
        "conditionals": [],
    }
