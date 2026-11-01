"""Convert a Great Tables GT object to gridwell IR JSON."""

from __future__ import annotations

import datetime as dt
import json
import re
from typing import Any

import numpy as np
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
    thead = _build_thead(all_cols, has_stub, built._stubhead)

    # Build tbody (with row groups)
    tbody = _build_tbody(tbl_data, all_cols, built, has_stub, has_groups, group_rows)

    # Build footer
    footer = _build_footer(built)

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
    config = {
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
    # A locale set on the table (`GT(locale=...)`) is its language; the
    # direction follows from the language, since the IR doesn't infer it.
    locale = getattr(getattr(built, "_locale", None), "_locale", None)
    if locale:
        lang = locale.replace("_", "-")
        config["locale"] = lang
        config["lang"] = lang
        if _is_rtl(lang):
            config["dir"] = "rtl"
    return config


# Scripts written right to left, and languages written right to left when the
# tag names no script (as in gridwell_core::lang).
_RTL_SCRIPTS = frozenset(
    "Adlm Arab Aran Hebr Mand Mend Nkoo Rohg Samr Syrc Thaa Yezi".split()
)
_RTL_LANGUAGES = frozenset(
    "ar arc bal bqi ckb dv fa glk he iw ji ks lrc mzn nqo pnb prs ps sd sdh syr ug ur yi".split()
)


def _is_rtl(lang: str) -> bool:
    """Whether a language tag's text is written right to left."""
    subtags = lang.split("-")
    if len(subtags) > 1 and len(subtags[1]) == 4 and subtags[1].isalpha():
        return subtags[1].title() in _RTL_SCRIPTS
    return subtags[0].lower() in _RTL_LANGUAGES


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


def _build_thead(all_cols: list, has_stub: bool, stubhead: Any) -> dict:
    """Build the thead section with column labels.

    The stub column's label is the stubhead's (`tab_stubhead()`), blank without
    one, as Great Tables shows it.
    """
    cells = []
    for i, col in enumerate(all_cols):
        if has_stub and i == 0:
            label = _stubhead_label(stubhead)
        else:
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


def _stubhead_label(stubhead: Any) -> str:
    """The stubhead's label text, or "" when there is none."""
    if isinstance(stubhead, list):
        stubhead = stubhead[0] if stubhead else None
    return "" if stubhead is None else str(stubhead)


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
        # The text Great Tables renders (`str()` of the built body), missing values
        # included: GT shows those as text too ("None", "nan").
        missing = bool(pd.isna(value))
        text = str(value)

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
            # A missing value sorts as missing, not as its text.
            "sort_key": text.lower() if text and not missing else None,
        }

        if not is_stub and text and not missing and not pd.isna(orig_val):
            typed = _typed_value(orig_val, text, data_type)
            if typed is not None:
                cell["typed_value"] = typed

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

    # Heuristic from value (a bool is an int, and a datetime a date)
    if isinstance(value, (bool, np.bool_)):
        return "boolean"
    elif isinstance(value, (int, np.integer)):
        return "integer"
    elif isinstance(value, (float, np.floating)):
        return "number"
    elif isinstance(value, dt.datetime):
        return "datetime"
    elif isinstance(value, dt.date):
        return "date"
    elif isinstance(value, dt.time):
        return "time"

    return "string"


# A number as displayed: an optional minus, digits, an optional fraction and
# exponent (thousands separators and U+2212 MINUS SIGN removed first).
_PLAIN_NUMBER = re.compile(r"-?[0-9]+(\.[0-9]+)?([eE][-+]?[0-9]+)?")


def _typed_value(value: Any, text: str, data_type: str | None) -> dict | None:
    """The cell's typed value, as the R emitter gives it.

    A number is the number displayed, so a spreadsheet holds what the table
    states (0.11, not 0.1111); a display that isn't a plain number (`$49.95`,
    `<0.001`) has none. Dates and times are ISO 8601 strings, without an offset
    (a spreadsheet has no time zone: the value is the wall-clock time).
    Anything else is a string, the text shown.
    """
    if data_type in ("number", "integer"):
        shown = text.replace("\u2212", "-").replace(",", "")
        if not _PLAIN_NUMBER.fullmatch(shown):
            return None
        if data_type == "integer" and not re.search(r"[.eE]", shown):
            return {"type": "integer", "value": int(shown)}
        return {"type": "number", "value": float(shown)}
    if data_type == "boolean":
        return {"type": "boolean", "value": bool(value)}
    if data_type == "datetime":
        stamp = pd.Timestamp(value)
        if stamp.tzinfo is not None:
            stamp = stamp.tz_localize(None)
        return {"type": "datetime", "value": stamp.isoformat()}
    if data_type == "date" and isinstance(value, dt.date):
        return {"type": "date", "value": value.isoformat()}
    if data_type == "time" and isinstance(value, dt.time):
        return {"type": "time", "value": value.replace(tzinfo=None).isoformat()}
    return {"type": "string", "value": text}


def _build_footer(built: Any) -> dict:
    """Build the footer section.

    The footnotes are the ones Great Tables lists under the table, in its order and
    with its marks (one per distinct text; footnotes of hidden columns dropped).
    Before 1.0 Great Tables stored footnotes but didn't show them, so neither do we.
    """
    try:
        from great_tables._utils_render_html import _process_footnotes_for_display
    except ImportError:
        shown = []
    else:
        shown = _process_footnotes_for_display(built, built._footnotes)

    fn_list = []
    for i, fn in enumerate(shown):
        fn_list.append({
            "id": f"fn{i + 1}",
            "mark": fn["mark"],
            "content": [{"type": "text", "value": _note_text(fn["text"])}],
        })

    sn_list = []
    for sn in built._source_notes:
        sn_list.append({
            "content": [{"type": "text", "value": _note_text(sn)}],
        })

    return {
        "footnotes": fn_list,
        "source_notes": sn_list,
    }


def _note_text(note: Any) -> str:
    """A note's text: a string as given, a Text (md(), html()) as Great Tables renders it."""
    if isinstance(note, str):
        return note
    from great_tables._text import _process_text

    return _process_text(note)


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
