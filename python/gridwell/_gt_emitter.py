"""gridwell's own copy of the Great Tables emitter: a GT object to gridwell IR.

Used when the installed Great Tables has no `GT.as_ir()` (see `_gt`). It reads
Great Tables internals, which is why Great Tables is taking it over.
"""

from __future__ import annotations

import datetime as dt
import html
import re
from html.parser import HTMLParser
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any

import numpy as np
import pandas as pd

if TYPE_CHECKING:
    from great_tables import GT


def build_ir(gt_obj: GT) -> dict[str, Any]:
    """The IR of a GT object, as a dict."""
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

    # Summary rows need a stub for their labels: Great Tables adds one when the
    # table has none (empty in the data rows), and so does the IR.
    summaries = _has_summaries(built)
    if not has_stub and summaries:
        stub_col_info = SimpleNamespace(
            var=_SUMMARY_STUB, column_label="", column_align="left", column_width=None
        )
        has_stub = True

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

    # Styled text runs (from markdown or HTML labels) register their styles here.
    registry = _StyleRegistry()

    # Build column_spec
    column_spec = _build_column_spec(all_cols, has_stub)

    # Build header
    header = _build_header(built._heading, registry)

    # Build thead
    thead = _build_thead(all_cols, has_stub, built, registry)

    # Build config
    n_body_rows = len(tbl_data)
    config = _build_config(n_cols, len(thead["rows"]), n_body_rows, stub_cols, built)

    # Build tbody (with row groups)
    tbody = _build_tbody(tbl_data, all_cols, built, has_stub, has_groups, group_rows, registry)

    # Build footer
    footer = _build_footer(built, registry)

    # Build styles
    styles = _build_styles(built._styles)
    styles["defs"].update(registry.defs)

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


def _build_config(
    n_cols: int, n_head_rows: int, n_body_rows: int, stub_cols: int, built: Any
) -> dict:
    """Build the config section."""
    config = {
        "table_cols": n_cols,
        "header_rows": n_head_rows,
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
        label = _plain(col.column_label) if col.column_label else ""
        specs.append({
            "id": col.var,
            "align": col.column_align or "left",
            "align_char": None,
            "width": col.column_width or "auto",
            "min_width": None,
            "max_width": None,
            "style_id": None,
            "hidden": False,
            # The stub added for summary rows has no label (as the R emitter's).
            "label": "" if col.var == _SUMMARY_STUB else label or col.var,
        })
    return specs


def _build_header(heading: Any, styles: _StyleRegistry) -> dict:
    """Build the header section from GT Heading."""
    title = None
    subtitle = None

    if heading.title:
        title = {"content": _label_nodes(heading.title, styles)}
    if heading.subtitle:
        subtitle = {"content": _label_nodes(heading.subtitle, styles)}

    return {
        "title": title,
        "subtitle": subtitle,
        "extra_lines": [],
        "preheader_content": None,
    }


def _head_cell(content: list[dict], colspan: int = 1, is_placeholder: bool = False) -> dict:
    return {
        "content": content,
        "colspan": colspan,
        "rowspan": 1,
        "style_id": None,
        "is_stub": False,
        "is_placeholder": is_placeholder,
        "scope": None if is_placeholder else ("colgroup" if colspan > 1 else "col"),
        "sort_key": None,
        "data_type": None,
    }


def _build_thead(all_cols: list, has_stub: bool, built: Any, styles: _StyleRegistry) -> dict:
    """Build the thead section: spanner rows (the top level first), then the column labels.

    The stub column's label is the stubhead's (`tab_stubhead()`), blank without
    one, as Great Tables shows it.
    """
    cells = []
    for i, col in enumerate(all_cols):
        if has_stub and i == 0:
            label = _stubhead_label(built._stubhead)
        else:
            label = col.column_label if col.column_label else col.var
        cells.append(_head_cell(_label_nodes(label, styles)))

    rows = _spanner_rows(all_cols, has_stub, built, styles)
    rows.append({
        "role": "column_label",
        "style_id": None,
        "cells": cells,
    })
    return {"rows": rows}


def _stubhead_label(stubhead: Any) -> Any:
    """The stubhead's label, or "" when there is none."""
    if isinstance(stubhead, list):
        stubhead = stubhead[0] if stubhead else None
    return "" if stubhead is None else stubhead


def _spanner_rows(all_cols: list, has_stub: bool, built: Any, styles: _StyleRegistry) -> list:
    """One spanner row per spanner level, the top level first, as Great Tables
    draws them: a spanner covers a run of adjacent columns with the same label,
    followed by placeholder cells. In the lowest spanner row each column without
    a spanner is a blank cell of its own (the label row's cell reaches up to it in
    Great Tables' HTML); in the rows above, adjacent columns without a spanner
    share one blank cell.
    """
    from great_tables._spanners import spanners_print_matrix

    levels, _ = spanners_print_matrix(
        spanners=built._spanners, boxhead=built._boxhead, ids=False, omit_columns_row=True
    )
    data_vars = [col.var for col in all_cols[1 if has_stub else 0:]]
    rows = []
    for level, by_var in enumerate(levels):
        lowest = level == len(levels) - 1
        labels = [by_var.get(var) for var in data_vars]
        cells = [_head_cell([])] if has_stub else []
        j = 0
        while j < len(labels):
            label = labels[j]
            blank = label is None or label == ""
            span = 1
            while j + span < len(labels) and _same_spanner(label, labels[j + span], lowest):
                span += 1
            content = [] if blank else _label_nodes(label, styles)
            cells.append(_head_cell(content, colspan=span))
            cells.extend(_head_cell([], is_placeholder=True) for _ in range(span - 1))
            j += span
        rows.append({"role": "spanner_label", "style_id": None, "cells": cells})
    return rows


def _same_spanner(a: Any, b: Any, lowest: bool) -> bool:
    """Whether two adjacent columns' spanner labels are one spanner cell."""
    if lowest:
        return a is not None and b is not None and a == b
    return ("" if a is None else a) == ("" if b is None else b)


def _build_tbody(
    tbl_data: pd.DataFrame,
    all_cols: list,
    built: Any,
    has_stub: bool,
    has_groups: bool,
    group_rows: Any,
    styles: _StyleRegistry,
) -> list[dict]:
    """Build the tbody section with row groups, then the grand summary (as a group
    of summary rows alone)."""
    grand = _summary_rows(built._summary_rows_grand, None, all_cols, "grand_summary_row", styles)
    grand_group = (
        [{"group_id": None, "label": None, "rows": [], "summary_rows": grand}] if grand else []
    )
    if has_groups and group_rows:
        groups = []
        for grp in group_rows:
            rows = []
            for idx in grp.indices:
                row = _build_row(tbl_data, int(idx), all_cols, built, has_stub)
                rows.append(row)

            groups.append({
                "group_id": grp.group_id,
                "label": _group_label(grp, styles),
                "rows": rows,
                "summary_rows": _summary_rows(
                    built._summary_rows, grp.group_id, all_cols, "summary_row", styles
                ),
            })
        return groups + grand_group
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
        }] + grand_group


def _group_label(grp: Any, styles: _StyleRegistry) -> dict | None:
    """A row group's label: its label as Great Tables shows it (HTML when a
    `groupname_col` formatter made it), else its id."""
    label = grp.group_label
    if label is None:
        label = grp.group_id
    elif isinstance(label, str):
        label = _HtmlLabel(label)
    content = _label_nodes(label, styles)
    if not content:
        return None
    return {"content": content, "style_id": None, "colspan": None}


# ─── Summary rows ───

# The stub column the IR gets when a table with summary rows has no stub of its own.
_SUMMARY_STUB = "::rowname::"


def _has_summaries(built: Any) -> bool:
    return bool(getattr(built, "_summary_rows", None) or getattr(built, "_summary_rows_grand", None))


def _summary_rows(
    summaries: Any, group_id: str | None, all_cols: list, role: str, styles: _StyleRegistry
) -> list[dict]:
    """A group's summary rows (the grand summary's for `group_id=None`): the label
    in the stub, then the text Great Tables shows for each column. Summary rows
    on a group's top side (`side="top"`) are listed with the others: the IR has
    summary rows only after a group's data rows."""
    if not summaries:
        return []
    found = (
        summaries.get_summary_rows()
        if group_id is None
        else summaries.get_summary_rows(group_id=group_id)
    )
    rows = []
    for summary in found:
        cells = []
        for i, col in enumerate(all_cols):
            # The table has a stub (one is added for summary rows when it has none).
            is_stub = i == 0
            if is_stub:
                content = _label_nodes(summary.id, styles)
            else:
                # Formatted text, which Great Tables puts in its HTML as it is.
                content = _label_nodes(_HtmlLabel(str(summary.values.get(col.var))), styles)
            cells.append({
                "content": content,
                "colspan": 1,
                "rowspan": 1,
                "style_id": None,
                "is_stub": is_stub,
                "is_placeholder": False,
                "scope": "row" if is_stub else None,
                "sort_key": None,
                "data_type": None,
            })
        rows.append({"role": role, "style_id": None, "cells": cells})
    return rows


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
        if col.var == _SUMMARY_STUB:
            # The stub added for summary rows is empty in the data rows.
            cells.append({
                "content": [],
                "colspan": 1,
                "rowspan": 1,
                "style_id": None,
                "is_stub": True,
                "is_placeholder": False,
                "data_type": None,
                "sort_key": None,
            })
            continue
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


def _build_footer(built: Any, styles: _StyleRegistry) -> dict:
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
        # The text is HTML already: Great Tables renders it before listing it.
        text = fn["text"]
        fn_list.append({
            "id": f"fn{i + 1}",
            "mark": fn["mark"],
            "content": _label_nodes(_HtmlLabel(text) if isinstance(text, str) else text, styles),
        })

    sn_list = []
    for sn in built._source_notes:
        sn_list.append({
            "content": _label_nodes(sn, styles),
        })

    return {
        "footnotes": fn_list,
        "source_notes": sn_list,
    }


# ─── Labels as content nodes ───
#
# A label (title, column label, spanner, group, footnote, source note) is what
# Great Tables puts in its HTML: a string escaped, `md()` rendered to HTML, `html()`
# as is. That HTML is lowered to the content nodes the IR draws: text, styled text
# runs and line breaks.


class _HtmlLabel:
    """A label that is HTML already (Great Tables puts it in its HTML as it is)."""

    def __init__(self, html: str):
        self.html = html


class _StyleRegistry:
    """Style definitions for styled text runs, deduplicated: identical
    definitions share an id (`gw_1`, `gw_2`, …, as the R emitter's)."""

    def __init__(self) -> None:
        self.defs: dict[str, dict[str, str]] = {}
        self._ids: dict[tuple, str] = {}

    def id(self, style: dict[str, str]) -> str:
        key = tuple(sorted(style.items()))
        if key not in self._ids:
            new = f"gw_{len(self.defs) + 1}"
            self.defs[new] = dict(key)
            self._ids[key] = new
        return self._ids[key]


def _label_html(label: Any) -> str:
    """A label's HTML, as Great Tables renders it."""
    if label is None:
        return ""
    if isinstance(label, _HtmlLabel):
        return label.html
    from great_tables._text import _process_text

    try:
        return _process_text(label)
    except TypeError:
        # Not a string or a Text: whatever else has HTML (a UnitStr), else its text.
        to_html = getattr(label, "to_html", None)
        return to_html() if callable(to_html) else html.escape(str(label))


def _label_nodes(label: Any, styles: _StyleRegistry) -> list[dict]:
    """A label's content nodes."""
    lines = _lower_html(_label_html(label))
    nodes: list[dict] = []
    for k, line in enumerate(lines):
        if k > 0:
            nodes.append({"type": "line_break"})
        for text, style in line:
            if style:
                nodes.append({"type": "styled_text", "value": text, "style_id": styles.id(style)})
            else:
                nodes.append({"type": "text", "value": text})
    return nodes


def _plain(label: Any) -> str:
    """A label's plain text (for `column_spec` labels): its lines joined by spaces."""
    return " ".join("".join(text for text, _ in line) for line in _lower_html(_label_html(label)))


# Tags that style the text inside them, and how.
_TAG_STYLES = {
    **dict.fromkeys(("b", "strong"), ("font_weight", "bold")),
    **dict.fromkeys(("i", "em", "cite", "var", "dfn"), ("font_style", "italic")),
    **dict.fromkeys(("u", "ins"), ("text_decoration", "underline")),
    **dict.fromkeys(("s", "strike", "del"), ("text_decoration", "line-through")),
    **dict.fromkeys(("code", "kbd", "samp", "tt"), ("font_family", "monospace")),
}
# Tags that start a line of their own, and tags whose contents aren't shown.
_BLOCKS = frozenset(
    "address article aside blockquote dd div dl dt figcaption figure footer h1 h2 h3 h4 h5 h6 "
    "header hr li main nav ol p pre section table tr ul".split()
)
_HIDDEN = frozenset(("script", "style", "template", "title", "head"))
_VOID = frozenset(
    "area base br col embed hr img input link meta source track wbr".split()
)
# White space a browser collapses (not U+00A0 NO-BREAK SPACE).
_SPACES = re.compile(r"[ \t\n\r\f]+")


class _Lowerer(HTMLParser):
    """HTML as lines of (text, style) runs, white space collapsed as a browser does."""

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.lines: list[list[tuple[str, dict[str, str]]]] = [[]]
        self.open: list[str] = []
        self.hidden = 0
        self.block_break = False

    def _style(self) -> dict[str, str]:
        style: dict[str, str] = {}
        for tag in self.open:
            if tag in _TAG_STYLES:
                prop, value = _TAG_STYLES[tag]
                style[prop] = value
        return style

    def _break(self) -> None:
        self.lines.append([])

    def _has_text(self) -> bool:
        return any(_SPACES.sub("", t) for t, _ in self.lines[-1])

    def handle_starttag(self, tag: str, attrs: Any) -> None:
        if tag == "br":
            # After a block, a <br> is a line of its own.
            if self.block_break and self._has_text():
                self._break()
            self._break()
            self.block_break = False
            return
        if tag in _BLOCKS:
            self.block_break = True
        if tag in _VOID:
            return
        self.open.append(tag)
        if tag in _HIDDEN:
            self.hidden += 1

    def handle_startendtag(self, tag: str, attrs: Any) -> None:
        self.handle_starttag(tag, attrs)
        if tag not in _VOID:
            self.handle_endtag(tag)

    def handle_endtag(self, tag: str) -> None:
        if tag in _BLOCKS:
            self.block_break = True
        # Close up to the matching tag; a stray end tag closes nothing.
        for i in range(len(self.open) - 1, -1, -1):
            if self.open[i] == tag:
                self.hidden -= sum(t in _HIDDEN for t in self.open[i:])
                del self.open[i:]
                return

    def handle_data(self, data: str) -> None:
        if self.hidden or not data:
            return
        if self.block_break:
            # A block starts a line, unless the line is empty so far.
            if self._has_text():
                self._break()
            self.block_break = False
        self.lines[-1].append((data, self._style()))


def _lower_html(markup: str) -> list[list[tuple[str, dict[str, str]]]]:
    """HTML as lines of merged (text, style) runs: white space runs one space,
    each line trimmed, empty lines at the start and end dropped."""
    lowerer = _Lowerer()
    lowerer.feed(markup)
    lowerer.close()
    lines = []
    for raw in lowerer.lines:
        runs: list[tuple[str, dict[str, str]]] = []
        for text, style in raw:
            text = _SPACES.sub(" ", text)
            # One space between runs: the first one's.
            if runs and runs[-1][0].endswith(" ") and text.startswith(" "):
                text = text[1:]
            if not runs and text.startswith(" "):
                text = text[1:]
            if not text:
                continue
            if runs and runs[-1][1] == style:
                runs[-1] = (runs[-1][0] + text, style)
            else:
                runs.append((text, style))
        if runs:
            last, style = runs[-1]
            last = last.rstrip(" ")
            if last:
                runs[-1] = (last, style)
            else:
                runs.pop()
        lines.append(runs)
    while lines and not lines[0]:
        lines.pop(0)
    while lines and not lines[-1]:
        lines.pop()
    return lines


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
