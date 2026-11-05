"""gridwell's own copy of the Great Tables emitter: a GT object to gridwell IR.

Used when the installed Great Tables has no `GT.as_ir()` (see `_gt`). It reads
Great Tables internals, which is why Great Tables is taking it over.

Styles and footnote marks follow Great Tables' HTML (`_utils_render_html`): each
part of the table gets the `tab_style()` styles and the marks Great Tables draws
on it, and the table options' styles (`_OptionStyles`), as the R emitter does
for gt. Labels and body cells are the content nodes of the HTML Great Tables puts
in them (`_lower_html`), so `fmt_*()` output that is HTML (`fmt_markdown()`,
`fmt_units()`, `fmt_image()`, ...) is drawn as it is formatted.
"""

from __future__ import annotations

import copy
import datetime as dt
import html
import json
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

    # Row grouping
    group_rows = built._stub.group_rows if hasattr(built._stub, "group_rows") else None
    has_groups = group_rows is not None and len(group_rows) > 0

    # Column info: the stub columns (`_Stub`), then the data columns
    visible_cols = built._boxhead._get_default_columns()
    stub = _stub_layout(built, has_groups)
    all_cols = stub.cols + list(visible_cols)

    n_cols = len(all_cols)
    stub_cols = len(stub.cols)

    # Styled text runs (from markdown or HTML labels), `tab_style()` styles and
    # table options register their styles here.
    registry = _StyleRegistry()
    shown_footnotes = _shown_footnotes(built)
    decor = _Decor(built, shown_footnotes, registry)

    # Build column_spec
    column_spec = _build_column_spec(all_cols, stub_cols)

    # Build header
    header = _build_header(built._heading, decor)

    # Build thead
    thead = _build_thead(all_cols, stub, built, decor)

    # Build config
    n_body_rows = len(tbl_data)
    config = _build_config(n_cols, len(thead["rows"]), n_body_rows, stub_cols, built)

    # Build tbody (with row groups)
    tbody = _build_tbody(tbl_data, all_cols, built, stub, has_groups, group_rows, decor)

    # Build footer
    footer = _build_footer(built, shown_footnotes, decor)

    return {
        "ir_version": "1.0",
        "config": config,
        "styles": {
            "defs": registry.defs,
            "compositions": {},
            "conditionals": decor.options.conditionals,
        },
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
    # (Great Tables' stripe isn't the IR's default one: striping is a conditional
    # style, `_option_styles`, and `row_striping` stays off.)
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


class _Stub:
    """The table's stub columns, as Great Tables lays them out: the row group
    column (`row_group_as_column=True`), then the stub columns, outermost first
    (a hierarchical stub, `rowname_col=[...]`, has several), the primary last."""

    def __init__(self, group: Any, rows: list[Any]):
        self.group = group
        self.rows = rows

    @property
    def cols(self) -> list[Any]:
        return ([self.group] if self.group is not None else []) + self.rows

    @property
    def primary(self) -> Any:
        return self.rows[-1] if self.rows else None


def _stub_layout(built: Any, has_groups: bool) -> _Stub:
    """The stub columns Great Tables draws (its `_get_stub_layout()`).

    Summary rows need a stub for their labels: Great Tables adds one when the
    table has none (empty in the data rows), and so does the IR. With a row group
    column and no stub, it adds none: the summary labels are in the group column
    (Great Tables puts a group's there too, a cell more than the table has)."""
    boxhead = built._boxhead
    get_stub_columns = getattr(boxhead, "_get_stub_columns", None)
    if callable(get_stub_columns):
        rows = list(get_stub_columns())
    else:
        rows = [c for c in [boxhead._get_stub_column()] if c is not None]
    summaries = _has_summaries(built)
    try:
        layout = built._stub._get_stub_layout(
            has_summary_rows=summaries, options=built._options, boxhead=boxhead
        )
    except (AttributeError, TypeError):
        layout = ["rowname"] if rows or summaries else []

    group = None
    if "group_label" in layout and has_groups:
        get_group = getattr(boxhead, "_get_row_group_column", None)
        found = get_group() if callable(get_group) else None
        group = SimpleNamespace(
            var=getattr(found, "var", None) or _GROUP_STUB,
            column_label=getattr(found, "column_label", None),
            column_align="left",
            column_width=getattr(found, "column_width", None),
        )
    if not rows and "rowname" in layout:
        rows = [
            SimpleNamespace(var=_SUMMARY_STUB, column_label="", column_align="left", column_width=None)
        ]
    return _Stub(group, rows)


# The group column's id when Great Tables has no column for the groups.
_GROUP_STUB = "::group::"


def _build_column_spec(all_cols: list, n_stub: int) -> list[dict]:
    """Build column_spec array from ColInfo objects. The stub columns are
    left-aligned, as Great Tables draws them whatever their data."""
    specs = []
    for i, col in enumerate(all_cols):
        label = _plain(col.column_label) if col.column_label else ""
        specs.append({
            "id": col.var,
            "align": "left" if i < n_stub else col.column_align or "left",
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


def _build_header(heading: Any, decor: _Decor) -> dict:
    """Build the header section from GT Heading."""
    title = None
    subtitle = None

    if heading.title:
        title = {
            "content": decor.with_marks(heading.title, decor.notes("LocTitle")),
            "style_id": decor.style_id(decor.styles("LocHeader") + decor.styles("LocTitle")),
        }
    if heading.subtitle:
        subtitle = {
            "content": decor.with_marks(heading.subtitle, decor.notes("LocSubTitle")),
            "style_id": decor.style_id(decor.styles("LocHeader") + decor.styles("LocSubTitle")),
        }

    return {
        "title": title,
        "subtitle": subtitle,
        "extra_lines": [],
        "preheader_content": None,
    }


def _head_cell(
    content: list[dict], colspan: int = 1, is_placeholder: bool = False, style_id: str | None = None
) -> dict:
    return {
        "content": content,
        "colspan": colspan,
        "rowspan": 1,
        "style_id": style_id,
        "is_stub": False,
        "is_placeholder": is_placeholder,
        "scope": None if is_placeholder else ("colgroup" if colspan > 1 else "col"),
        "sort_key": None,
        "data_type": None,
    }


def _build_thead(all_cols: list, stub: _Stub, built: Any, decor: _Decor) -> dict:
    """Build the thead section: spanner rows (the top level first), then the column labels.

    The stub columns' label is the stubhead's (`tab_stubhead()`), blank without
    one, across all of them and left-aligned, as Great Tables shows it; given a label for each
    stub column, each has its own (and a row group column a blank one), the
    first with the footnote marks. The table options for the column labels (fill,
    weight) style every head row, under any `tab_style()` on the cells.
    """
    n_stub = len(stub.cols)
    cells = []
    if n_stub:
        labels = _stubhead_labels(built._stubhead, len(stub.rows))
        notes = decor.notes("LocStubhead")
        style_id = decor.style_id(decor.styles("LocStubhead"))
        if len(labels) == 1:
            # Left-aligned across the stub, where a cell across columns is centered.
            across = decor.style_id(
                decor.styles("LocStubhead"), {"text_align": "left"} if n_stub > 1 else {}
            )
            cells.append(
                _head_cell(decor.with_marks(labels[0], notes), colspan=n_stub, style_id=across)
            )
            cells.extend(_head_cell([], is_placeholder=True) for _ in range(n_stub - 1))
        else:
            if stub.group is not None:
                cells.append(_head_cell([], style_id=style_id))
            for k, label in enumerate(labels):
                cells.append(
                    _head_cell(decor.with_marks(label, notes if k == 0 else []), style_id=style_id)
                )
    for col in all_cols[n_stub:]:
        label = col.column_label if col.column_label else col.var
        notes = decor.notes("LocColumnLabels", lambda x, var=col.var: x.colname == var)
        styles = decor.styles("LocColumnHeader") + decor.styles(
            "LocColumnLabels", lambda x, var=col.var: x.colname == var
        )
        cells.append(_head_cell(decor.with_marks(label, notes), style_id=decor.style_id(styles)))

    rows = _spanner_rows(all_cols, n_stub, built, decor, cells)
    rows.append({
        "role": "column_label",
        "style_id": None,
        "cells": cells,
    })
    for row in rows:
        row["style_id"] = decor.options.head_row
    return {"rows": rows}


def _stubhead_labels(stubhead: Any, n_rows: int) -> list[Any]:
    """The stubhead's labels: one across the stub ("" when there is none), or one
    per stub column when `tab_stubhead()` was given a label for each."""
    if isinstance(stubhead, list):
        if 1 < len(stubhead) == n_rows:
            return list(stubhead)
        stubhead = stubhead[0] if stubhead else None
    return ["" if stubhead is None else stubhead]


def _spanner_rows(
    all_cols: list, n_stub: int, built: Any, decor: _Decor, label_cells: list[dict]
) -> list:
    """One spanner row per spanner level, the top level first, as Great Tables
    draws them: a spanner covers a run of adjacent columns with the same label,
    followed by placeholder cells. In the lowest spanner row each column without
    a spanner is a blank cell of its own (the label row's cell reaches up to it in
    Great Tables' HTML), and so is each stub column; in the rows above, adjacent
    columns without a spanner share one blank cell, and the stub columns do.

    A spanner's styles and footnotes are found by its id. A blank cell has the
    column headings' styles (`loc.column_header()`), as Great Tables' blank cells
    do; one the label below reaches up to in Great Tables (the stubhead's, or a
    column's in the lowest row) has that label's fill instead.
    """
    from great_tables._spanners import spanners_print_matrix

    levels, _ = spanners_print_matrix(
        spanners=built._spanners, boxhead=built._boxhead, ids=False, omit_columns_row=True
    )
    id_levels, _ = spanners_print_matrix(
        spanners=built._spanners, boxhead=built._boxhead, ids=True, omit_columns_row=True
    )
    offset = n_stub
    data_vars = [col.var for col in all_cols[offset:]]
    header_styles = decor.styles("LocColumnHeader")

    def reached(cell: dict) -> str | None:
        fill = decor.registry.get(cell["style_id"]).get("background_color")
        return decor.style_id([], {"background_color": fill} if fill else {})

    # The stubhead cell above each stub column (a placeholder's is the one across the stub).
    stubhead = [
        label_cells[0] if label_cells[k]["is_placeholder"] else label_cells[k]
        for k in range(n_stub)
    ]

    rows = []
    for level, (by_var, ids_by_var) in enumerate(zip(levels, id_levels)):
        lowest = level == len(levels) - 1
        labels = [by_var.get(var) for var in data_vars]
        ids = [ids_by_var.get(var) for var in data_vars]
        cells = []
        if n_stub and lowest:
            cells.extend(_head_cell([], style_id=reached(cell)) for cell in stubhead)
        elif n_stub:
            cells.append(_head_cell([], colspan=n_stub, style_id=decor.style_id(header_styles)))
            cells.extend(_head_cell([], is_placeholder=True) for _ in range(n_stub - 1))
        j = 0
        while j < len(labels):
            label, span_id = labels[j], ids[j]
            blank = label is None or label == ""
            span = 1
            while j + span < len(labels) and _same_spanner(label, labels[j + span], lowest):
                span += 1
            if blank:
                content: list[dict] = []
                style_id = (
                    reached(label_cells[offset + j]) if lowest else decor.style_id(header_styles)
                )
            else:
                def ours(x: Any, span_id: Any = span_id) -> bool:
                    return bool(span_id) and span_id in x.grpname

                content = decor.with_marks(
                    label,
                    decor.notes("LocSpannerLabels", lambda x, s=span_id: bool(s) and x.grpname == s),
                )
                style_id = decor.style_id(header_styles + decor.styles("LocSpannerLabels", ours))
            cells.append(_head_cell(content, colspan=span, style_id=style_id))
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
    stub: _Stub,
    has_groups: bool,
    group_rows: Any,
    decor: _Decor,
) -> list[dict]:
    """Build the tbody section with row groups, then the grand summary (as a group
    of summary rows alone). Data rows are counted across groups, for striping, as
    Great Tables counts them.

    With a row group column, a group's label is the cell in that column of its
    first row, spanning its data rows (the IR has no span from a group's rows
    into its summary rows), and the group has no label of its own."""
    grand = _summary_rows(
        built._summary_rows_grand, None, all_cols, stub, "grand_summary_row", decor
    )
    grand_group = (
        [{"group_id": None, "label": None, "rows": [], "summary_rows": grand}] if grand else []
    )
    if has_groups and group_rows:
        listed = [(grp, [int(idx) for idx in grp.indices]) for grp in group_rows]
    else:
        listed = [(None, list(range(len(tbl_data))))]
    spans = _stub_spans(tbl_data, stub, listed)

    groups = []
    shown = 0
    for grp, indices in listed:
        group_cell = None
        if grp is not None and stub.group is not None:
            group_cell = _group_cell(grp, len(indices), shown, decor)
        rows = []
        for k, idx in enumerate(indices):
            lead = None
            if group_cell is not None:
                lead = group_cell if k == 0 else _body_placeholder(is_stub=True)
            rows.append(
                _build_row(tbl_data, idx, shown, all_cols, built, stub, decor, spans[shown], lead)
            )
            shown += 1
        if grp is None:
            groups.append({"group_id": None, "label": None, "rows": rows, "summary_rows": []})
            continue
        groups.append({
            "group_id": grp.group_id,
            "label": None if group_cell is not None else _group_label(grp, decor),
            "rows": rows,
            "summary_rows": _summary_rows(
                built._summary_rows, grp.group_id, all_cols, stub, "summary_row", decor,
                group_style=None if group_cell is None else group_cell["style_id"],
            ),
        })
    return groups + grand_group


def _stub_spans(
    tbl_data: pd.DataFrame, stub: _Stub, listed: list[tuple[Any, list[int]]]
) -> list[list[tuple[int, list[int]]]]:
    """For each data row as shown, each stub column's (rowspan, rows covered): a
    rowspan of 0 for a cell covered by one above.

    In a hierarchical stub, as Great Tables merges it: an outer column's cells
    merge across adjacent rows of a group with the same text in that column and
    every column to its left. The primary column's never do."""
    order = [(idx, k) for k, (_, indices) in enumerate(listed) for idx in indices]
    names = [col.var for col in stub.rows]
    spans = [[(1, [idx]) for _ in names] for idx, _ in order]
    if len(names) < 2:
        return spans
    values = [[str(tbl_data[var].iloc[idx]) for var in names] for idx, _ in order]
    for level in range(len(names) - 1):
        start = 0
        for j in range(1, len(order) + 1):
            if (
                j < len(order)
                and order[j][1] == order[start][1]
                and values[j][: level + 1] == values[start][: level + 1]
            ):
                continue
            spans[start][level] = (j - start, [order[t][0] for t in range(start, j)])
            for t in range(start + 1, j):
                spans[t][level] = (0, [])
            start = j
    return spans


def _body_placeholder(is_stub: bool) -> dict:
    """A body cell covered by a span."""
    return {
        "content": [],
        "colspan": 1,
        "rowspan": 1,
        "style_id": None,
        "is_stub": is_stub,
        "is_placeholder": True,
        "scope": None,
        "data_type": None,
        "sort_key": None,
    }


def _group_label_of(grp: Any) -> Any:
    """A row group's label as Great Tables shows it: HTML when a `groupname_col`
    formatter made it, else its id."""
    label = grp.group_label
    if label is None:
        return grp.group_id
    return _HtmlLabel(label) if isinstance(label, str) else label


def _group_label(grp: Any, decor: _Decor) -> dict | None:
    """A row group's label (`_group_label_of`). The table options for group
    labels (fill, weight) are under any `tab_style()` on it."""
    gid = grp.group_id
    content = decor.with_marks(
        _group_label_of(grp), decor.notes("LocRowGroups", lambda x: x.grpname == gid)
    )
    if not content:
        return None
    styles = decor.styles("LocRowGroups", lambda x: x.grpname is not None and gid in x.grpname)
    return {
        "content": content,
        "style_id": decor.style_id(styles, decor.options.group_label),
        "colspan": None,
    }


def _group_cell(grp: Any, n_rows: int, shown: int, decor: _Decor) -> dict:
    """A row group's label in the row group column, the cell of its first data
    row (the `shown`-th shown) spanning its `n_rows` rows. Its styles are the
    group label's, under the table options for the row group column (not the
    group labels' or the stub's), and it isn't striped, as Great Tables draws it."""
    gid = grp.group_id
    content = decor.with_marks(
        _group_label_of(grp), decor.notes("LocRowGroups", lambda x: x.grpname == gid)
    )
    styles = decor.styles("LocRowGroups", lambda x: x.grpname is not None and gid in x.grpname)
    return {
        "content": content,
        "colspan": 1,
        "rowspan": n_rows,
        "style_id": decor.style_id(styles, decor.options.group_cell(shown)),
        "is_stub": True,
        "is_placeholder": False,
        "scope": "rowgroup",
        "data_type": None,
        "sort_key": None,
    }


# ─── Summary rows ───

# The stub column the IR gets when a table with summary rows has no stub of its own.
_SUMMARY_STUB = "::rowname::"


def _has_summaries(built: Any) -> bool:
    return bool(getattr(built, "_summary_rows", None) or getattr(built, "_summary_rows_grand", None))


def _summary_rows(
    summaries: Any,
    group_id: str | None,
    all_cols: list,
    stub: _Stub,
    role: str,
    decor: _Decor,
    group_style: str | None = None,
) -> list[dict]:
    """A group's summary rows (the grand summary's for `group_id=None`): the label
    across the stub, then the text Great Tables shows for each column. Summary
    rows on a group's top side (`side="top"`) are listed with the others: the IR
    has summary rows only after a group's data rows.

    With a row group column, a group's summary label is across the other stub
    columns, as Great Tables draws it under the group's label; the group column
    is an empty cell there, styled as the group's label (`group_style`), since
    the IR has no span from a group's rows into its summary rows. A grand
    summary label is across the whole stub, the group column too.

    Styles and footnotes are found as Great Tables finds them: by the row's index
    among its group's rows on the same side (a grand summary row's among all of
    them, the top ones first), and a footnote also by its group (a style isn't).
    The label has the stub's table options under the row's, as Great Tables
    draws it."""
    if not summaries:
        return []
    grand = group_id is None
    found = summaries.get_summary_rows() if grand else summaries.get_summary_rows(group_id=group_id)
    cell_loc, stub_loc = (
        ("LocGrandSummary", "LocGrandSummaryStub") if grand else ("LocSummary", "LocSummaryStub")
    )
    row_style = decor.options.grand_summary_row if grand else decor.options.summary_row
    n_top = sum(getattr(s, "side", "bottom") == "top" for s in found)
    # The table has a stub (one is added for summary rows when it has none).
    n_stub = len(stub.cols)
    first = 1 if stub.group is not None and stub.rows and not grand else 0
    label_var = all_cols[first].var
    rows = []
    for k, summary in enumerate(found):
        index = k if grand or k < n_top else k - n_top

        def in_group(x: Any) -> bool:
            return grand or x.grpname == group_id

        cells = []
        if first:
            cells.append({
                "content": [],
                "colspan": 1,
                "rowspan": 1,
                "style_id": group_style,
                "is_stub": True,
                "is_placeholder": False,
                "scope": None,
                "sort_key": None,
                "data_type": None,
            })
        if stub.group is not None:
            styles = decor.styles(stub_loc, lambda x: x.rownum == index)
        else:
            styles = decor.styles(
                cell_loc, lambda x: x.rownum == index and x.colname == label_var
            ) + decor.styles(
                stub_loc,
                lambda x: (x.colname is None or x.colname == label_var) and x.rownum == index,
            )
        stub_opts = decor.options.stub
        under = [stub_opts, decor.registry.get(row_style)] if stub_opts else []
        span = n_stub - first
        cells.append({
            "content": decor.with_marks(
                summary.id, decor.notes(stub_loc, lambda x: x.rownum == index and in_group(x))
            ),
            "colspan": span,
            "rowspan": 1,
            "style_id": decor.style_id(styles, *under),
            "is_stub": True,
            "is_placeholder": False,
            "scope": "row",
            "sort_key": None,
            "data_type": None,
        })
        cells.extend(_body_placeholder(is_stub=True) for _ in range(span - 1))
        for col in all_cols[n_stub:]:
            var = col.var
            # Formatted text, which Great Tables puts in its HTML as it is.
            notes = decor.notes(
                cell_loc, lambda x: x.colname == var and x.rownum == index and in_group(x)
            )
            cells.append({
                "content": decor.with_marks(_HtmlLabel(str(summary.values.get(var))), notes),
                "colspan": 1,
                "rowspan": 1,
                "style_id": decor.style_id(
                    decor.styles(cell_loc, lambda x: x.rownum == index and x.colname == var)
                ),
                "is_stub": False,
                "is_placeholder": False,
                "scope": None,
                "sort_key": None,
                "data_type": None,
            })
        rows.append({"role": role, "style_id": row_style, "cells": cells})
    return rows


def _build_row(
    tbl_data: pd.DataFrame,
    row_idx: int,
    shown: int,
    all_cols: list,
    built: Any,
    stub: _Stub,
    decor: _Decor,
    spans: list[tuple[int, list[int]]],
    lead: dict | None = None,
) -> dict:
    """Build a single row: data row `row_idx`, the `shown`-th shown (from 0),
    its stub columns' `spans` (`_stub_spans`), and `lead`, its cell in the row
    group column (if any).

    Styles and footnotes are found by the row's index in the data, as Great
    Tables finds them. A stub cell spanning rows has the stub's (`loc.stub()`)
    for its column in any of them (a style once), and those for no column in
    its first; footnotes for no column are on the primary column."""
    cells = [] if lead is None else [lead]
    n_stub = len(stub.cols)
    for i, col in enumerate(all_cols):
        if lead is not None and i == 0:
            continue
        is_stub = i < n_stub
        var = col.var
        styles = decor.styles("LocBody", lambda x: x.rownum == row_idx and x.colname == var)
        rowspan = 1
        if is_stub:
            rowspan, covered = spans[i - (n_stub - len(stub.rows))]
            if rowspan == 0:
                cells.append(_body_placeholder(is_stub=True))
                continue
            primary = col is stub.primary
            notes = decor.notes(
                "LocStub",
                lambda x: (x.colname is None and primary and x.rownum == row_idx)
                or (x.colname == var and x.rownum in covered),
            )
            labels = decor.styles(
                "LocStub",
                lambda x: (x.colname is None and x.rownum == row_idx)
                or (x.colname == var and x.rownum in covered),
            )
            if len(covered) > 1:
                labels = list({id(x.styles): x for x in labels}.values())
            styles += labels
        else:
            notes = decor.notes("LocBody", lambda x: x.colname == var and x.rownum == row_idx)
        style_id = decor.style_id(styles, decor.options.data_cell(is_stub, shown))
        if col.var == _SUMMARY_STUB:
            # The stub added for summary rows is empty in the data rows.
            cells.append({
                "content": decor.with_marks("", notes),
                "colspan": 1,
                "rowspan": 1,
                "style_id": style_id,
                "is_stub": True,
                "is_placeholder": False,
                "scope": "row",
                "data_type": None,
                "sort_key": None,
            })
            continue
        value = tbl_data[col.var].iloc[row_idx]
        # The HTML Great Tables puts in the cell (`str()` of the built body: a
        # formatted value, or the data escaped), missing values included: GT shows
        # those as text too ("None", "nan").
        missing = bool(pd.isna(value))
        label = _HtmlLabel(str(value))
        text = _plain(label)

        # Determine data type from the original data
        orig_val = built._tbl_data[col.var].iloc[row_idx]
        data_type = _infer_data_type(orig_val, col.var, built._tbl_data.dtypes)

        cell = {
            "content": decor.with_marks(label, notes),
            "colspan": 1,
            "rowspan": rowspan,
            "style_id": style_id,
            "is_stub": is_stub,
            "is_placeholder": False,
            "data_type": data_type,
            # A missing value sorts as missing, not as its text; the text shown
            # sorts, not its markup.
            "sort_key": text.lower() if text and not missing else None,
        }

        if is_stub:
            # A stub cell heads its row, or the rows it spans (as Great Tables has it).
            cell["scope"] = "rowgroup" if rowspan > 1 else "row"
        if not is_stub and text and not missing and not pd.isna(orig_val):
            typed = _typed_value(orig_val, text, data_type)
            if typed is not None:
                cell["typed_value"] = typed

        cells.append(cell)

    return {
        "role": None,
        "style_id": decor.options.body_row,
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


def _shown_footnotes(built: Any) -> list[dict[str, Any]]:
    """The footnotes Great Tables lists under the table, in its order and with its
    marks (one per distinct text; footnotes of hidden columns dropped). Before 1.0
    Great Tables stored footnotes but didn't show them, so neither do we."""
    try:
        from great_tables._utils_render_html import _process_footnotes_for_display
    except ImportError:
        return []
    return list(_process_footnotes_for_display(built, built._footnotes))


def _build_footer(built: Any, shown: list[dict[str, Any]], decor: _Decor) -> dict:
    """Build the footer section: the footnotes shown (`_shown_footnotes`), then the
    source notes. Each footnote is styled by `loc.footer()` and `loc.footnotes()`,
    each source note by `loc.footer()` and `loc.source_notes()` (only when they're
    on lines of their own, as Great Tables styles them)."""
    fn_style = decor.style_id(decor.styles("LocFooter") + decor.styles("LocFootnotes"))
    fn_list = []
    for i, fn in enumerate(shown):
        # The text is HTML already: Great Tables renders it before listing it.
        text = fn["text"]
        fn_list.append({
            "id": f"fn{i + 1}",
            "mark": fn["mark"],
            "content": _label_nodes(
                _HtmlLabel(text) if isinstance(text, str) else text, decor.registry
            ),
            "style_id": fn_style,
        })

    multiline = getattr(getattr(built._options, "source_notes_multiline", None), "value", True)
    sn_style = (
        decor.style_id(decor.styles("LocFooter") + decor.styles("LocSourceNotes"))
        if multiline
        else None
    )
    sn_list = []
    for sn in built._source_notes:
        sn_list.append({
            "content": _label_nodes(sn, decor.registry),
            "style_id": sn_style,
        })

    return {
        "footnotes": fn_list,
        "source_notes": sn_list,
    }


# ─── Labels and cells as content nodes ───
#
# A label (title, column label, spanner, group, footnote, source note) is what
# Great Tables puts in its HTML: a string escaped, `md()` rendered to HTML, `html()`
# as is. So is a body cell's text: `fmt_*()` output in the html context
# (`fmt_markdown()`, `fmt_units()`, `fmt_image()`, ...), anything else escaped.
# That HTML is lowered to the content nodes the IR draws: text, styled text runs,
# line breaks, images, and raw HTML for an SVG (an icon, a flag, a nanoplot).


class _HtmlLabel:
    """A label that is HTML already (Great Tables puts it in its HTML as it is)."""

    def __init__(self, html: str):
        self.html = html


class _StyleRegistry:
    """Style definitions, deduplicated: identical definitions share an id
    (`gw_1`, `gw_2`, …, as the R emitter's)."""

    def __init__(self) -> None:
        self.defs: dict[str, dict[str, Any]] = {}
        self._ids: dict[str, str] = {}

    def id(self, style: dict[str, Any]) -> str:
        key = json.dumps(style, sort_keys=True)
        if key not in self._ids:
            new = f"gw_{len(self.defs) + 1}"
            self.defs[new] = json.loads(key)
            self._ids[key] = new
        return self._ids[key]

    def get(self, style_id: str | None) -> dict[str, Any]:
        """The definition of `style_id` (empty for None)."""
        return {} if style_id is None else copy.deepcopy(self.defs[style_id])


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
        for run in line:
            if isinstance(run, dict):
                nodes.append(run)
                continue
            text, style = run
            if style:
                nodes.append({"type": "styled_text", "value": text, "style_id": styles.id(style)})
            else:
                nodes.append({"type": "text", "value": text})
    return nodes


def _plain(label: Any) -> str:
    """A label's plain text (for `column_spec` labels, a cell's sort key): its
    lines joined by spaces, without its images and SVGs."""
    return " ".join(
        "".join(run[0] for run in line if isinstance(run, tuple)).strip(" ")
        for line in _lower_html(_label_html(label))
    ).strip(" ")


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
# The Unicode superscript and subscript characters, as gridwell's writers draw
# `<sup>` and `<sub>` (`gridwell_layout::text`): digits, `+ - − = ( )`, and the
# lowercase letters that have one.
_SCRIPTS = {
    "sup": dict(zip(
        "0123456789+-−=()abcdefghijklmnoprstuvwxyz",
        "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁻⁼⁽⁾ᵃᵇᶜᵈᵉᶠᵍʰⁱʲᵏˡᵐⁿᵒᵖʳˢᵗᵘᵛʷˣʸᶻ",
    )),
    "sub": dict(zip(
        "0123456789+-−=()aehijklmnoprstuvx",
        "₀₁₂₃₄₅₆₇₈₉₊₋₋₌₍₎ₐₑₕᵢⱼₖₗₘₙₒₚᵣₛₜᵤᵥₓ",
    )),
}
# An image size the IR takes: a non-negative length, a percentage or `auto`.
_IMAGE_SIZE = re.compile(r"auto|0|\+?(\d+\.?\d*|\.\d+)(px|pt|rem|em|in|cm|mm|%)", re.I)

# A line of lowered HTML: (text, style) runs and content nodes (images, raw SVG).
_Line = list[Any]


class _Lowerer(HTMLParser):
    """HTML as lines of runs, white space collapsed as a browser does. A run is
    (text, style), or a content node: an image for an `<img>`, raw HTML for an
    `<svg>` (its markup as written)."""

    def __init__(self, markup: str) -> None:
        super().__init__(convert_charrefs=True)
        self.markup = markup
        # Where each line of `markup` starts (`getpos()` is a line and column).
        self.line_starts = [0] + [m.end() for m in re.finditer("\n", markup)]
        self.lines: list[_Line] = [[]]
        # Open tags, each with where a `<sup>`/`<sub>`'s runs start (line, run).
        self.open: list[tuple[str, tuple[int, int] | None]] = []
        self.hidden = 0
        self.block_break = False
        # Where an `<svg>` being skipped starts in `markup`, and how deep it is.
        self.svg_start: int | None = None
        self.svg_depth = 0

    def _offset(self) -> int:
        line, col = self.getpos()
        return self.line_starts[line - 1] + col

    def _style(self) -> dict[str, str]:
        style: dict[str, str] = {}
        for tag, _ in self.open:
            if tag in _TAG_STYLES:
                prop, value = _TAG_STYLES[tag]
                style[prop] = value
        return style

    def _break(self) -> None:
        self.lines.append([])

    def _has_text(self) -> bool:
        return any(
            isinstance(run, dict) or _SPACES.sub("", run[0]) for run in self.lines[-1]
        )

    def _add(self, run: Any) -> None:
        if self.block_break:
            # A block starts a line, unless the line is empty so far.
            if self._has_text():
                self._break()
            self.block_break = False
        self.lines[-1].append(run)

    def _raw(self, markup: str) -> None:
        self._add({"type": "raw", "format": "html", "value": markup})

    def handle_starttag(self, tag: str, attrs: Any) -> None:
        if self.svg_start is not None:
            self.svg_depth += tag == "svg"
            return
        if tag == "svg" and not self.hidden:
            self.svg_start, self.svg_depth = self._offset(), 1
            return
        if tag == "br":
            # After a block, a <br> is a line of its own.
            if self.block_break and self._has_text():
                self._break()
            self._break()
            self.block_break = False
            return
        if tag in _BLOCKS:
            self.block_break = True
        if tag == "img" and not self.hidden:
            self._add(_image(attrs))
        if tag in _VOID:
            return
        start = (len(self.lines) - 1, len(self.lines[-1])) if tag in _SCRIPTS else None
        self.open.append((tag, start))
        if tag in _HIDDEN:
            self.hidden += 1

    def handle_startendtag(self, tag: str, attrs: Any) -> None:
        if tag == "svg" and self.svg_start is None and not self.hidden:
            self._raw(self.get_starttag_text() or "")
            return
        self.handle_starttag(tag, attrs)
        if tag not in _VOID:
            self.handle_endtag(tag)

    def handle_endtag(self, tag: str) -> None:
        if self.svg_start is not None:
            if tag == "svg":
                self.svg_depth -= 1
                if not self.svg_depth:
                    end = self.markup.find(">", self._offset())
                    end = len(self.markup) if end < 0 else end + 1
                    self._raw(self.markup[self.svg_start:end])
                    self.svg_start = None
            return
        if tag in _BLOCKS:
            self.block_break = True
        # Close up to the matching tag; a stray end tag closes nothing.
        for i in range(len(self.open) - 1, -1, -1):
            if self.open[i][0] == tag:
                self._close(i)
                return

    def _close(self, at: int) -> None:
        """Close the open tags from `at` up, innermost first."""
        while len(self.open) > at:
            tag, start = self.open.pop()
            self.hidden -= tag in _HIDDEN
            if start is not None:
                self._script(_SCRIPTS[tag], *start)

    def _script(self, chars: dict[str, str], line: int, first: int) -> None:
        """Redraw the runs from `first` in superscript or subscript characters, if
        they're all on one line, all text, and every character has one (white
        space stays); leave them as written otherwise."""
        if line != len(self.lines) - 1:
            return
        runs = self.lines[line][first:]
        if any(isinstance(run, dict) for run in runs):
            return
        drawn = []
        for text, style in runs:
            if any(c not in chars and not _SPACES.match(c) for c in text):
                return
            drawn.append(("".join(chars.get(c, c) for c in text), style))
        self.lines[line][first:] = drawn

    def handle_data(self, data: str) -> None:
        if self.hidden or self.svg_start is not None or not data:
            return
        self._add((data, self._style()))

    def close(self) -> None:
        super().close()
        if self.svg_start is not None:
            # An `<svg>` never closed runs to the end.
            self._raw(self.markup[self.svg_start:])
            self.svg_start = None
        self._close(0)


def _image(attrs: Any) -> dict[str, str]:
    """An `<img>`'s image node: its `src` and `alt`, and its `width` and `height`
    (a `style` attribute's over the attributes, as a browser sizes it; a number is
    pixels), those the IR can take. Only what the tag says: nothing is read."""
    given: dict[str, str] = {}
    for name, value in attrs:
        given.setdefault(name, value or "")
    node = {"type": "image", "src": given.get("src", "").strip()}
    if "alt" in given:
        node["alt"] = given["alt"]
    sizes = {}
    for side in ("width", "height"):
        value = given.get(side, "").strip()
        sizes[side] = value + "px" if _BARE_NUMBER.fullmatch(value) else value
    for prop, value in _DECLARATION.findall(given.get("style", "")):
        prop = prop.lower()
        if prop in sizes:
            sizes[prop] = re.sub(r"\s*!important$", "", value.strip(), flags=re.I)
    for side, value in sizes.items():
        if _IMAGE_SIZE.fullmatch(value):
            node[side] = value
    return node


def _lower_html(markup: str) -> list[_Line]:
    """HTML as lines of merged runs: white space runs one space, each line
    trimmed, empty lines at the start and end dropped. A content node keeps the
    spaces around it, as an inline image does."""
    lowerer = _Lowerer(markup)
    lowerer.feed(markup)
    lowerer.close()
    lines = []
    for raw in lowerer.lines:
        runs: _Line = []
        for run in raw:
            if isinstance(run, dict):
                runs.append(run)
                continue
            text, style = run
            text = _SPACES.sub(" ", text)
            last = runs[-1] if runs else None
            # One space between runs (the first one's), none at the start.
            if text.startswith(" ") and (last is None or (isinstance(last, tuple) and last[0].endswith(" "))):
                text = text[1:]
            if not text:
                continue
            if isinstance(last, tuple) and last[1] == style:
                runs[-1] = (last[0] + text, style)
            else:
                runs.append((text, style))
        if runs and isinstance(runs[-1], tuple):
            text, style = runs[-1]
            text = text.rstrip(" ")
            if text:
                runs[-1] = (text, style)
            else:
                runs.pop()
        lines.append(runs)
    while lines and not lines[0]:
        lines.pop(0)
    while lines and not lines[-1]:
        lines.pop()
    return lines


# ─── Styles and footnote marks by location ───
#
# Great Tables finds a part of the table's `tab_style()` styles and footnotes as
# it draws the part (`_utils_render_html`): by the kind of location (`loc.*`) and,
# for most kinds, the column, row or group. `_Decor` finds them the same way,
# and adds the table options' styles (`_OptionStyles`).


def _loc_classes(name: str) -> tuple[type, ...]:
    """Great Tables' location class `name`; none if this version has no such location."""
    from great_tables import _locations

    cls = getattr(_locations, name, None)
    return (cls,) if isinstance(cls, type) else ()


class _Decor:
    """The styles and footnote marks of each part of the table."""

    def __init__(self, built: Any, shown_footnotes: list[dict[str, Any]], registry: _StyleRegistry):
        self.built = built
        self.registry = registry
        self._styles = list(getattr(built, "_styles", None) or [])
        # Footnotes placed somewhere (one without a location is listed, not marked).
        self._notes = [
            f for f in getattr(built, "_footnotes", None) or [] if f.locname is not None and f.footnotes
        ]
        # The footer's footnote of each mark.
        self._refs = {
            fn["mark"]: f"fn{i + 1}" for i, fn in enumerate(shown_footnotes) if fn["mark"]
        }
        self.options = _OptionStyles(built, registry)

    def styles(self, loc_name: str, pred: Any = None) -> list:
        """The `tab_style()` entries at a kind of location (and matching `pred`)."""
        classes = _loc_classes(loc_name)
        return [
            x for x in self._styles
            if isinstance(x.locname, classes) and (pred is None or pred(x))
        ]

    def notes(self, loc_name: str, pred: Any = None) -> list:
        """The footnotes at a kind of location (and matching `pred`)."""
        classes = _loc_classes(loc_name)
        return [
            x for x in self._notes
            if isinstance(x.locname, classes) and (pred is None or pred(x))
        ]

    def style_id(self, styles: list, *under: dict[str, Any]) -> str | None:
        """The id of the definitions `under` (each over the ones before it), overlaid
        by `styles` as the CSS Great Tables writes for them in the cell's `style`
        attribute (in order, later declarations winning); None if all are empty."""
        css = " ".join(el._to_html_style() for x in styles for el in x.styles)
        merged: dict[str, Any] = {}
        for part in (*under, _css_style(css)):
            merged = _merge_style(merged, part)
        return self.registry.id(merged) if merged else None

    def with_marks(self, label: Any, notes: list, nodes: list[dict] | None = None) -> list[dict]:
        """A label's content nodes (`nodes`, if given) with the marks of `notes`, as
        Great Tables places them: each mark once, numbers in order, before the text
        for `placement="left"` and after it for "right"; for "auto" (the default)
        before a number and after anything else. Where a location has several
        footnotes, the first one's placement counts. A mark's text has a comma
        after it when another follows."""
        if nodes is None:
            nodes = _label_nodes(label, self.registry)
        if not notes:
            return nodes
        from great_tables._utils_render_html import (
            _get_footnote_mark_string,
            _get_footnote_marks_option,
            _is_numeric_content,
        )

        marks: list[str] = []
        first = None
        for fn in notes:
            mark = _get_footnote_mark_string(self.built, fn)
            if mark not in marks:
                marks.append(mark)
                if first is None:
                    first = fn
        if _get_footnote_marks_option(self.built) == "numbers":
            marks.sort(key=lambda m: int(m) if m.isdigit() else float("inf"))
        marks = [m for m in marks if m in self._refs]
        if not marks:
            return nodes
        mark_nodes = [
            {
                "type": "footnote_mark",
                "ref": self._refs[m],
                "mark_text": m + ("," if k < len(marks) - 1 else ""),
            }
            for k, m in enumerate(marks)
        ]
        placement = getattr(getattr(first, "placement", None), "name", "auto")
        if placement == "left":
            left = True
        elif placement == "right":
            left = False
        else:
            left = _is_numeric_content(_label_html(label))
        return mark_nodes + nodes if left else nodes + mark_nodes


class _OptionStyles:
    """The table options a user changed (`tab_options()`, `opt_stylize()`,
    `opt_row_striping()`) that the IR can express, as the R emitter's: style ids
    for the head rows, data rows and summary rows, definitions for group labels,
    the row group column and stub cells (under any `tab_style()` on them), and
    the striping. Options at
    Great Tables' defaults are left out: gridwell's writers supply their own
    default look. A fill comes with the text color Great Tables gives it.

    Great Tables' stripe (#F4F4F4 by default) isn't the IR's default one, so
    striping is always a conditional style on even data rows (Great Tables
    stripes the 2nd, 4th, … counted across groups). It covers the stub and the
    body, so the cells of the part not striped (`row_striping_include_stub`,
    `row_striping_include_table_body`) get the table's background on those rows,
    and stub cells with a fill of their own get the stripe where they're striped
    (Great Tables' stripe wins over the stub's fill)."""

    def __init__(self, built: Any, registry: _StyleRegistry):
        options = built._options
        try:
            from great_tables._gt_data import Options

            defaults: Any = Options()
        except ImportError:
            defaults = None

        def raw(name: str) -> Any:
            return getattr(getattr(options, name, None), "value", None)

        def value(name: str) -> Any:
            v = raw(name)
            if v is None or v == getattr(getattr(defaults, name, None), "value", None):
                return None
            return v

        def fill(name: str) -> dict[str, Any]:
            bg = value(name)
            return {} if bg is None else self._fill(str(bg))

        def weight(name: str) -> dict[str, Any]:
            w = value(name)
            return {} if w is None else {"font_weight": str(w)}

        def id_of(style: dict[str, Any]) -> str | None:
            return registry.id(style) if style else None

        self._dark = raw("table_font_color") or "#333333"
        self._light = raw("table_font_color_light") or "#FFFFFF"
        self.head_row = id_of(
            {**fill("column_labels_background_color"), **weight("column_labels_font_weight")}
        )
        self.group_label = {**fill("row_group_background_color"), **weight("row_group_font_weight")}
        self.stub = {**fill("stub_background_color"), **weight("stub_font_weight")}
        self._stub_group = {
            **fill("stub_row_group_background_color"),
            **weight("stub_row_group_font_weight"),
        }
        self.body_row = id_of(
            {"border": {"top": {"style": "none"}}}
            if value("table_body_hlines_style") == "none"
            else {}
        )
        self.summary_row = id_of(fill("summary_row_background_color"))
        self.grand_summary_row = id_of(fill("grand_summary_row_background_color"))

        self.conditionals: list[dict[str, Any]] = []
        self._striped = (
            bool(raw("row_striping_include_stub")),
            bool(raw("row_striping_include_table_body")),
        )
        self._stripe: dict[str, Any] = {}
        self._unstriped: dict[str, Any] = {}
        color = raw("row_striping_background_color")
        if any(self._striped) and color:
            self._stripe = self._fill(str(color))
            self._unstriped = self._fill(str(raw("table_background_color") or "#FFFFFF"))
            self.conditionals.append({
                "id": "row_striping",
                "selector": {"row_parity": "even", "scope": "tbody"},
                "style": self._stripe,
            })

    def _fill(self, background: str) -> dict[str, Any]:
        """A fill, with the text color Great Tables gives it (the table's dark or
        light text color, whichever contrasts more)."""
        style: dict[str, Any] = {"background_color": background}
        try:
            from great_tables._scss import font_color

            style["color"] = font_color(background, self._dark, self._light)
        except Exception:
            # A color Great Tables can't read: no text color for it.
            pass
        return style

    def data_cell(self, is_stub: bool, shown: int) -> dict[str, Any]:
        """The options' definition for a data row's cell, the `shown`-th row shown."""
        base = self.stub if is_stub else {}
        if self._stripe and shown % 2 == 1:
            if self._striped[0 if is_stub else 1]:
                if base:
                    base = _merge_style(base, self._stripe)
            else:
                base = _merge_style(self._unstriped, base)
        return base

    def group_cell(self, shown: int) -> dict[str, Any]:
        """The options' definition for a row group column's cell, starting in the
        `shown`-th row shown (never striped)."""
        if self._stripe and shown % 2 == 1:
            return _merge_style(self._unstriped, self._stub_group)
        return self._stub_group


def _merge_style(base: dict[str, Any], over: dict[str, Any]) -> dict[str, Any]:
    """`base` with `over`'s properties over it (nested ones, borders and padding,
    property by property)."""
    out = copy.deepcopy(base)
    for key, value in over.items():
        if isinstance(value, dict) and isinstance(out.get(key), dict):
            out[key] = _merge_style(out[key], value)
        else:
            out[key] = copy.deepcopy(value)
    return out


# CSS properties with an IR equivalent of the same value.
_CSS_PROPERTIES = {
    "color": "color",
    "background-color": "background_color",
    "font-family": "font_family",
    "font-size": "font_size",
    "font-weight": "font_weight",
    "font-style": "font_style",
    "text-align": "text_align",
    "vertical-align": "vertical_align",
    "text-transform": "text_transform",
    "text-decoration": "text_decoration",
    "white-space": "white_space",
    "text-indent": "indent",
    "word-break": "word_break",
    "overflow": "overflow",
    "text-overflow": "text_overflow",
    "min-width": "min_width",
    "max-width": "max_width",
}
_SIDES = ("top", "right", "bottom", "left")
_BORDER_STYLES = frozenset("none hidden dotted dashed solid double groove ridge inset outset".split())
_BORDER_WIDTH = re.compile(r"thin|medium|thick|[-+]?(\d+\.?\d*|\.\d+)([a-z%]+)?", re.I)
_BARE_NUMBER = re.compile(r"[-+]?(\d+\.?\d*|\.\d+)")
# A declaration, and a value's space-separated parts (quotes and parentheses kept whole).
_DECLARATION = re.compile(r"""([-a-zA-Z]+)\s*:\s*((?:[^;"'(]|"[^"]*"|'[^']*'|\([^)]*\))*)""")
_VALUE_PARTS = re.compile(r"""(?:[^\s"'(]|"[^"]*"|'[^']*'|\([^)]*\))+""")


def _sides(value: str) -> dict[str, str]:
    """A 1–4 value box shorthand (`padding: 1px 2px`) by side, as CSS reads it."""
    parts = _VALUE_PARTS.findall(value)
    if not 1 <= len(parts) <= 4:
        return {}
    top, right, bottom, left = {
        1: lambda p: (p[0],) * 4,
        2: lambda p: (p[0], p[1], p[0], p[1]),
        3: lambda p: (p[0], p[1], p[2], p[1]),
        4: lambda p: tuple(p),
    }[len(parts)](parts)
    return {"top": top, "right": right, "bottom": bottom, "left": left}


def _border(value: str) -> dict[str, str]:
    """A border shorthand (`2px solid #000`) as an IR border."""
    border: dict[str, str] = {}
    color = []
    for part in _VALUE_PARTS.findall(value):
        if part.lower() in _BORDER_STYLES:
            border["style"] = part.lower()
        elif _BORDER_WIDTH.fullmatch(part):
            border["width"] = part
        else:
            color.append(part)
    if color:
        border["color"] = " ".join(color)
    return border


def _css_style(css: str) -> dict[str, Any]:
    """CSS declarations (a `style` attribute's) as an IR style definition, later
    declarations winning. Properties the IR has no equivalent for (`font-stretch`,
    ...) are left out. A font size without a unit is in pixels (as Great Tables
    documents `style.text(size=)`); quotes around font names are dropped."""
    style: dict[str, Any] = {}
    for prop, value in _DECLARATION.findall(css):
        prop = prop.lower()
        value = re.sub(r"\s*!important$", "", value.strip(), flags=re.I)
        if not value:
            continue
        if prop in _CSS_PROPERTIES:
            if prop == "font-family":
                value = ", ".join(f.strip().strip("'\"").strip() for f in value.split(","))
            elif prop == "font-size" and _BARE_NUMBER.fullmatch(value):
                value += "px"
            style[_CSS_PROPERTIES[prop]] = value
            continue
        name = prop.split("-")
        if prop == "padding":
            style.setdefault("padding", {}).update(_sides(value))
        elif name[0] == "padding" and len(name) == 2 and name[1] in _SIDES:
            style.setdefault("padding", {})[name[1]] = value
        elif prop == "border":
            style["border"] = {side: _border(value) for side in _SIDES}
        elif name[0] == "border" and len(name) == 2 and name[1] in _SIDES:
            style.setdefault("border", {})[name[1]] = _border(value)
        elif name[0] == "border" and len(name) == 2 and name[1] in ("width", "style", "color"):
            for side, part in _sides(value).items():
                part = part.lower() if name[1] == "style" else part
                style.setdefault("border", {}).setdefault(side, {})[name[1]] = part
        elif (
            name[0] == "border" and len(name) == 3 and name[1] in _SIDES
            and name[2] in ("width", "style", "color")
        ):
            part = value.lower() if name[2] == "style" else value
            style.setdefault("border", {}).setdefault(name[1], {})[name[2]] = part
    # Empty borders (a shorthand of nothing the IR reads) and padding are left out.
    for key in ("border", "padding"):
        if key in style:
            style[key] = {k: v for k, v in style[key].items() if v}
            if not style[key]:
                del style[key]
    return style
