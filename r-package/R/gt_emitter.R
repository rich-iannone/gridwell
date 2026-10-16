#' Convert a gt table object to gridwell IR JSON
#'
#' Cell values come from gt's own formatting step (as they appear in gt's
#' rendered output, not the raw data). `md()` text becomes bold and italic
#' runs, spanners become spanner rows, footnote marks are placed in the cells
#' they annotate, and `tab_style()` styles become IR styles.
#'
#' @param gt_obj A `gt_tbl` object from the gt package.
#' @return A JSON string in gridwell IR format.
#' @export
gt_to_ir <- function(gt_obj) {
    ir <- gt_to_ir_list(gt_obj)
    jsonlite::toJSON(ir, auto_unbox = TRUE, null = "null", pretty = TRUE, digits = NA)
}

#' Convert a gt table object to a gridwell IR list
#'
#' @inheritParams gt_to_ir
#' @return A list representing the gridwell IR.
#' @export
gt_to_ir_list <- function(gt_obj) {
    if (!inherits(gt_obj, "gt_tbl")) {
        stop("`gt_obj` must be a gt table (a `gt_tbl` object).", call. = FALSE)
    }
    if (!requireNamespace("gt", quietly = TRUE)) {
        stop("Converting a gt table needs the gt package.", call. = FALSE)
    }

    built <- .gw_build_gt(gt_obj)
    styles <- .gw_style_registry()

    data <- built[["_data"]]
    body <- built[["_body"]]
    boxhead <- built[["_boxhead"]]
    stub_df <- built[["_stub_df"]]
    # The build turns the heading, column labels and source notes into plain text;
    # the unbuilt table still has their markdown.
    heading <- gt_obj[["_heading"]]

    # Visible columns: the stub (if any) first, then the default columns.
    stub_var <- boxhead$var[boxhead$type == "stub"]
    stub_var <- if (length(stub_var) > 0) stub_var[1] else NULL
    # Summary rows need a stub for their labels: gt adds one when the table has
    # none (empty in the data rows), and so does the IR.
    summaries <- .gw_summaries(built)
    if (is.null(stub_var) && length(summaries) > 0) stub_var <- .gw_summary_stub
    has_stub <- !is.null(stub_var)
    all_vars <- c(stub_var, boxhead$var[boxhead$type == "default"])
    opts <- .gw_option_styles(built, styles)

    marks <- .gw_footnote_marks(built, styles)
    cell_styles <- .gw_cell_styles(built, styles)

    column_spec <- lapply(all_vars, function(var) {
        idx <- which(boxhead$var == var)
        width <- if (length(idx) > 0) boxhead$column_width[[idx]] else NULL
        width <- if (is.null(width) || !nzchar(width[1])) "auto" else as.character(width[1])
        list(
            id = var,
            align = .gw_align(
                if (length(idx) > 0) boxhead$column_align[idx] else "left",
                is_stub = identical(var, stub_var)
            ),
            align_char = NULL,
            width = width,
            min_width = NULL,
            max_width = NULL,
            style_id = NULL,
            hidden = FALSE,
            label = .gw_plain(.gw_column_label(gt_obj, var, stub_var))
        )
    })

    # Header: title and subtitle
    header <- list(title = NULL, subtitle = NULL, extra_lines = list(), preheader_content = NULL)
    for (part in c("title", "subtitle")) {
        text <- heading[[part]]
        if (!is.null(text) && length(text) > 0 && !is.na(text[1]) && nzchar(text[1])) {
            header[[part]] <- list(
                content = .gw_with_marks(.gw_nodes(text, styles), marks$lookup(part)),
                style_id = cell_styles$lookup(part)
            )
        }
    }

    # Column headings: spanner rows (top level first), then the column labels.
    label_row <- list(
        role = "column_label",
        style_id = NULL,
        cells = lapply(all_vars, function(var) {
            key <- if (identical(var, stub_var)) "stubhead" else paste0("columns_columns:", var)
            .gw_cell(
                .gw_with_marks(
                    .gw_nodes(.gw_column_label(gt_obj, var, stub_var), styles),
                    marks$lookup(key)
                ),
                style_id = cell_styles$lookup(key),
                scope = "col"
            )
        })
    )
    thead_rows <- c(
        .gw_spanner_rows(built, all_vars, has_stub, styles, marks, cell_styles),
        list(label_row)
    )
    # Table options for the column labels (fill, weight) apply to every head row,
    # under any tab_style() on the cells.
    if (!is.null(opts$head_row)) {
        thead_rows <- lapply(thead_rows, function(r) {
            r$style_id <- opts$head_row
            r
        })
    }

    # Body rows, in gt's display order.
    rows <- lapply(seq_len(nrow(body)), function(i) {
        orig <- stub_df$rownum_i[i]
        cells <- lapply(all_vars, function(var) {
            is_stub_cell <- identical(var, stub_var)
            key <- if (is_stub_cell) paste0("stub:", i) else paste0("data:", i, ":", var)
            right <- !is_stub_cell &&
                identical(boxhead$column_align[boxhead$var == var], "right")
            shown <- body[[var]][i]
            missing <- length(shown) == 1 && is.na(shown)
            text <- .gw_body_text(shown)
            raw <- if (var %in% names(data)) data[[var]][orig] else NULL
            cell_style <- cell_styles$lookup(key)
            if (is_stub_cell) cell_style <- .gw_merge_style(styles, opts$stub, cell_style)
            cell <- .gw_cell(
                .gw_with_marks(.gw_text_nodes(text), marks$lookup(key), right_aligned = right),
                style_id = cell_style,
                is_stub = is_stub_cell,
                scope = if (is_stub_cell) "row" else NULL,
                data_type = .gw_dtype(raw),
                # A missing value sorts as missing, not as the text "NA".
                sort_key = if (nzchar(text) && !missing) tolower(text) else NULL
            )
            typed <- if (is_stub_cell) NULL else .gw_typed_value(raw, text)
            if (!is.null(typed)) cell$typed_value <- typed
            cell
        })
        list(role = NULL, style_id = opts$body_row, cells = cells)
    })

    # Before the styles are listed below: the body registers styles too.
    tbody <- .gw_tbody(
        built, rows, styles, marks, cell_styles, summaries, all_vars, stub_var, opts
    )

    # Footer: footnotes (in mark order) and source notes.
    footer <- list(footnotes = marks$footnotes, source_notes = list())
    source_notes <- gt_obj[["_source_notes"]]
    if (length(source_notes) > 0) {
        footer$source_notes <- lapply(source_notes, function(sn) {
            list(content = .gw_nodes(sn, styles), style_id = cell_styles$lookup("source_notes"))
        })
    }

    list(
        ir_version = "1.0",
        config = list(
            table_cols = length(all_vars),
            header_rows = length(thead_rows),
            body_rows = length(rows),
            stub_cols = if (has_stub) 1L else 0L,
            row_striping = opts$striping$row_striping,
            row_striping_include_stub = opts$striping$include_stub,
            row_striping_include_body = opts$striping$include_body,
            column_labels_hidden = FALSE,
            table_width = NULL,
            container_width = NULL,
            container_height = NULL,
            container_overflow = NULL,
            locale = "en-US",
            page_break_mode = "avoid",
            aria_label = NULL,
            aria_describedby = NULL,
            summary = NULL
        ),
        styles = list(
            defs = styles$defs(),
            compositions = setNames(list(), character(0)),
            conditionals = opts$striping$conditionals
        ),
        header = header,
        column_spec = column_spec,
        table = list(
            thead = list(rows = thead_rows),
            tbody = tbody
        ),
        footer = footer,
        extensions = setNames(list(), character(0))
    )
}

# ---- gt's build ---------------------------------------------------------------

# gt's own build (`gt:::build_data()`) in its plain-text ("grid") context, up to
# resolving footnotes and styles. The steps that paste footnote marks into the
# text are left out: the emitter places the marks as content nodes instead.
.gw_build_gt <- function(data) {
    step <- function(name) utils::getFromNamespace(name, "gt")
    context <- "grid"
    if (step("is_gt_tbl_empty")(data = data)) {
        data <- step("adjust_gt_tbl_empty")(data = data)
    }
    data <- step("dt_summary_cols_build")(data = data, context = context)
    data <- step("dt_body_build")(data = data)
    data <- step("render_formats")(data = data, skip_compat_check = FALSE, context = context)
    data <- step("render_substitutions")(data = data, context = context)
    data <- step("migrate_unformatted_to_output")(data = data, context = context)
    data <- step("perform_col_merge")(data = data, context = context)
    data <- step("reorder_stub_df")(data = data)
    data <- step("dt_body_reassemble")(data = data)
    data <- step("reorder_footnotes")(data = data)
    data <- step("reorder_styles")(data = data)
    data <- step("perform_text_transforms")(data = data)
    for (name in c(
        "dt_boxhead_build", "dt_spanners_build", "dt_heading_build", "dt_stubhead_build",
        "dt_stub_df_build", "dt_source_notes_build", "dt_summary_build", "dt_groups_rows_build"
    )) {
        data <- step(name)(data = data, context = context)
    }
    data <- step("resolve_deferred_styles")(data = data)
    data <- step("resolve_footnotes_styles")(data = data, tbl_type = "footnotes")
    data <- step("resolve_footnotes_styles")(data = data, tbl_type = "styles")
    data
}

# ---- Text and markdown ----------------------------------------------------------

# A label as gt holds it: markdown (`md()`), HTML (`html()`) or plain text.
.gw_nodes <- function(x, styles) {
    if (is.list(x) && !inherits(x, c("from_markdown", "html"))) {
        if (length(x) == 0) return(list())
        x <- x[[1]]
    }
    if (is.null(x) || length(x) == 0 || is.na(x[1])) return(list())
    if (inherits(x, "from_markdown")) return(.gw_md_nodes(as.character(x[1]), styles))
    if (inherits(x, "html")) return(.gw_text_nodes(.gw_strip_html(as.character(x[1]))))
    .gw_text_nodes(as.character(x[1]))
}

# Plain text, with newlines as line breaks.
.gw_text_nodes <- function(text) {
    if (!nzchar(text)) return(list())
    parts <- strsplit(text, "\n", fixed = TRUE)[[1]]
    nodes <- list()
    for (k in seq_along(parts)) {
        if (k > 1) nodes <- c(nodes, list(list(type = "line_break")))
        if (nzchar(parts[k])) nodes <- c(nodes, list(list(type = "text", value = parts[k])))
    }
    nodes
}

# The plain text of a label (for `column_spec$label`).
.gw_plain <- function(x) {
    nodes <- .gw_nodes(x, .gw_style_registry())
    text <- vapply(nodes, function(n) {
        if (identical(n$type, "line_break")) " " else n$value
    }, character(1))
    paste(text, collapse = "")
}

.gw_strip_html <- function(x) {
    x <- gsub("<br\\s*/?>", "\n", x, ignore.case = TRUE)
    x <- gsub("<[^>]+>", "", x)
    entities <- c(
        "&lt;" = "<", "&gt;" = ">", "&quot;" = "\"", "&#39;" = "'",
        "&nbsp;" = "\u00a0", "&amp;" = "&"
    )
    for (e in names(entities)) x <- gsub(e, entities[[e]], x, fixed = TRUE)
    x
}

# Inline markdown, the subset table labels use: `***bold italic***`, `**bold**`,
# `__bold__`, `*italic*`, `_italic_`, `` `code` ``, `[text](url)` and hard line
# breaks (two trailing spaces, a backslash, or `<br>`). A single newline is a
# space, as in markdown; backslash escapes are honored.
.gw_md_nodes <- function(md, styles) {
    md <- gsub("\r\n?", "\n", md)
    md <- gsub("( {2,}|\\\\)\n|<br\\s*/?>", "\001", md, ignore.case = TRUE, perl = TRUE)
    md <- gsub("\n", " ", md, fixed = TRUE)
    lines <- strsplit(md, "\001", fixed = TRUE)[[1]]
    nodes <- list()
    for (k in seq_along(lines)) {
        if (k > 1) nodes <- c(nodes, list(list(type = "line_break")))
        nodes <- c(nodes, .gw_md_inline(trimws(lines[k]), styles))
    }
    nodes
}

.gw_md_inline <- function(s, styles) {
    patterns <- c(
        bold_italic = "(?<!\\\\)\\*\\*\\*(.+?)\\*\\*\\*",
        bold = "(?<!\\\\)\\*\\*(.+?)\\*\\*",
        bold = "(?<![\\w\\\\])__(.+?)__(?!\\w)",
        italic = "(?<![\\\\*])\\*(?![\\s*])(.+?)(?<!\\s)\\*",
        italic = "(?<![\\w\\\\])_(?!\\s)(.+?)(?<!\\s)_(?!\\w)",
        code = "`([^`]+)`",
        link = "\\[([^\\]]*)\\]\\([^)]*\\)"
    )
    plain <- function(x) {
        x <- gsub("\\\\([\\\\`*_{}\\[\\]()#+.!|-])", "\\1", x, perl = TRUE)
        if (nzchar(x)) list(list(type = "text", value = x)) else list()
    }
    nodes <- list()
    while (nzchar(s)) {
        hits <- lapply(patterns, function(p) regexpr(p, s, perl = TRUE))
        starts <- vapply(hits, function(h) as.integer(h), integer(1))
        if (all(starts < 0)) {
            nodes <- c(nodes, plain(s))
            break
        }
        starts[starts < 0] <- .Machine$integer.max
        k <- which.min(starts)
        h <- hits[[k]]
        start <- as.integer(h)
        end <- start + attr(h, "match.length") - 1L
        cap <- attr(h, "capture.start")[1]
        inner <- substr(s, cap, cap + attr(h, "capture.length")[1] - 1L)
        nodes <- c(nodes, plain(substr(s, 1L, start - 1L)))
        kind <- names(patterns)[k]
        if (kind %in% c("code", "link")) {
            nodes <- c(nodes, plain(inner))
        } else {
            def <- switch(kind,
                bold_italic = list(font_style = "italic", font_weight = "bold"),
                bold = list(font_weight = "bold"),
                italic = list(font_style = "italic")
            )
            for (n in .gw_md_inline(inner, styles)) {
                if (identical(n$type, "text")) {
                    n <- list(type = "styled_text", value = n$value, style_id = styles$id(def))
                } else if (identical(n$type, "styled_text")) {
                    n$style_id <- styles$id(utils::modifyList(styles$get(n$style_id), def))
                }
                nodes <- c(nodes, list(n))
            }
        }
        s <- substr(s, end + 1L, nchar(s))
    }
    nodes
}

# A body value from gt's grid-context output.
.gw_body_text <- function(x) {
    if (is.null(x) || length(x) == 0) return("")
    # An unformatted missing value: gt shows it as "NA".
    if (is.na(x)) return("NA")
    as.character(x)
}

# ---- Labels, spanners and row groups ----------------------------------------------

# A column's label (or the stubhead label for the stub), from the unbuilt table.
.gw_column_label <- function(gt_obj, var, stub_var) {
    if (identical(var, stub_var)) {
        label <- gt_obj[["_stubhead"]]$label
        return(if (is.null(label) || length(label) == 0) "" else label)
    }
    boxhead <- gt_obj[["_boxhead"]]
    label <- boxhead$column_label[[which(boxhead$var == var)]]
    if (is.null(label)) var else label
}

# One spanner row per spanner level, the top level first. A spanner covers a run
# of adjacent columns, followed by placeholder cells.
.gw_spanner_rows <- function(built, all_vars, has_stub, styles, marks, cell_styles) {
    spanners <- built[["_spanners"]]
    if (is.null(spanners) || nrow(spanners) == 0) return(list())
    levels <- sort(unique(spanners$spanner_level), decreasing = TRUE)
    lapply(levels, function(level) {
        at_level <- spanners[spanners$spanner_level == level, , drop = FALSE]
        owner <- vapply(all_vars, function(var) {
            hit <- which(vapply(at_level$vars, function(v) var %in% v, logical(1)))
            if (length(hit) > 0) at_level$spanner_id[hit[1]] else NA_character_
        }, character(1))
        if (has_stub) owner[1] <- NA_character_
        cells <- list()
        j <- 1L
        while (j <= length(all_vars)) {
            id <- owner[[j]]
            span <- 1L
            while (!is.na(id) && j + span <= length(all_vars) &&
                identical(owner[[j + span]], id)) {
                span <- span + 1L
            }
            if (is.na(id)) {
                cells <- c(cells, list(.gw_cell(list(), scope = "col")))
            } else {
                key <- paste0("columns_groups:", id)
                label <- at_level$spanner_label[[which(at_level$spanner_id == id)[1]]]
                cells <- c(cells, list(.gw_cell(
                    .gw_with_marks(.gw_nodes(label, styles), marks$lookup(key)),
                    colspan = span,
                    style_id = cell_styles$lookup(key),
                    scope = if (span > 1) "colgroup" else "col"
                )))
                for (p in seq_len(span - 1L)) {
                    cells <- c(cells, list(.gw_cell(list(), is_placeholder = TRUE)))
                }
            }
            j <- j + span
        }
        list(role = "spanner_label", style_id = NULL, cells = cells)
    })
}

.gw_tbody <- function(built, rows, styles, marks, cell_styles, summaries, all_vars, stub_var,
                      opts) {
    stub_df <- built[["_stub_df"]]
    row_groups <- built[["_row_groups"]]
    summary_rows <- function(id, role, row_style) {
        df <- summaries[[id]]
        if (is.null(df)) return(list())
        lapply(seq_len(nrow(df)), function(i) {
            cells <- lapply(all_vars, function(var) {
                is_stub_cell <- identical(var, stub_var)
                text <- if (is_stub_cell) df[["::rowname::"]][i] else df[[var]][i]
                .gw_cell(
                    .gw_text_nodes(.gw_summary_text(text)),
                    is_stub = is_stub_cell,
                    scope = if (is_stub_cell) "row" else NULL
                )
            })
            list(role = role, style_id = row_style, cells = cells)
        })
    }
    grand <- summary_rows(.gw_grand_summary_id, "grand_summary_row", opts$grand_summary_row)
    grand_group <- if (length(grand) > 0) {
        list(list(group_id = NULL, label = NULL, rows = list(), summary_rows = grand))
    }
    if (length(row_groups) == 0 || all(is.na(stub_df$group_id))) {
        return(c(
            list(list(group_id = NULL, label = NULL, rows = rows, summary_rows = list())),
            grand_group
        ))
    }
    groups <- lapply(row_groups, function(grp_id) {
        idx <- which(stub_df$group_id == grp_id)
        label <- stub_df$group_label[[idx[1]]]
        if (is.null(label)) label <- grp_id
        key <- paste0("row_groups:", grp_id)
        list(
            group_id = grp_id,
            label = list(
                content = .gw_with_marks(.gw_nodes(label, styles), marks$lookup(key)),
                style_id = .gw_merge_style(styles, opts$group_label, cell_styles$lookup(key)),
                colspan = NULL
            ),
            rows = rows[idx],
            summary_rows = summary_rows(grp_id, "summary_row", opts$summary_row)
        )
    })
    c(groups, grand_group)
}

# ---- Summary rows -------------------------------------------------------------------

# gt's id for the grand summary in its built summary data, and the stub column the
# IR gets when a table with summaries has no stub of its own.
.gw_grand_summary_id <- "::GRAND_SUMMARY"
.gw_summary_stub <- "::rowname::"

# gt's formatted summary rows, by group id (`.gw_grand_summary_id` for the grand
# summary): data frames with `::rowname::` (the label) and one column per variable,
# holding the text gt displays. Summaries placed at the top of a group (`side =
# "top"`) are listed with the others: the IR has summary rows only after a group's
# data rows.
.gw_summaries <- function(built) {
    if (length(built[["_summary"]]) == 0) return(list())
    get <- utils::getFromNamespace("dt_summary_df_get", "gt")
    display <- get(data = built)$summary_df_display_list
    if (is.null(display)) return(list())
    lapply(display, as.data.frame, stringsAsFactors = FALSE)
}

.gw_summary_text <- function(x) {
    if (is.null(x) || length(x) == 0 || is.na(x)) return("")
    as.character(x)
}

# ---- Table options ------------------------------------------------------------------

# The table options a user changed (`tab_options()`, `opt_stylize()`,
# `opt_row_striping()`) that the IR can express, as style ids for each part plus
# the striping config. Options at gt's defaults are left out: gridwell's writers
# supply their own default look. Outer border colors and hairline colors have no
# IR equivalent yet.
.gw_option_styles <- function(built, styles) {
    options <- built[["_options"]]
    defaults <- gt::gt(data.frame(x = 1))[["_options"]]
    value <- function(name) {
        v <- options$value[[match(name, options$parameter)]]
        d <- defaults$value[[match(name, defaults$parameter)]]
        if (identical(v, d) || is.null(v) || length(v) == 0 || is.na(v[1])) NULL else v[1]
    }
    raw <- function(name) options$value[[match(name, options$parameter)]][1]
    ideal <- utils::getFromNamespace("ideal_fgnd_color", "gt")
    fill <- function(name) {
        bg <- value(name)
        if (is.null(bg)) return(list())
        # Text color for the fill, chosen as gt does.
        fg <- unname(ideal(
            bg, light = raw("table_font_color_light"), dark = raw("table_font_color")
        ))
        list(background_color = bg, color = fg)
    }
    weight <- function(name) {
        w <- value(name)
        if (is.null(w)) list() else list(font_weight = as.character(w))
    }
    id_of <- function(def) if (length(def) == 0) NULL else styles$id(def)

    head_row <- c(fill("column_labels_background_color"), weight("column_labels_font_weight"))
    group_label <- c(fill("row_group_background_color"), weight("row_group_font_weight"))
    stub <- c(fill("stub_background_color"), weight("stub_font_weight"))
    body_row <- if (identical(value("table_body_hlines_style"), "none")) {
        list(border = list(top = list(style = "none")))
    } else {
        list()
    }

    striping <- list(
        row_striping = FALSE, include_stub = FALSE, include_body = FALSE,
        conditionals = list()
    )
    include_body <- isTRUE(raw("row_striping_include_table_body"))
    include_stub <- isTRUE(raw("row_striping_include_stub"))
    if (include_body || include_stub) {
        color <- value("row_striping_background_color")
        if (is.null(color)) {
            # gt's default stripe is the IR's.
            striping$row_striping <- TRUE
            striping$include_body <- include_body
            striping$include_stub <- include_stub
        } else {
            # A custom stripe: a conditional style on even data rows (gt stripes
            # the 2nd, 4th, … rows). It also covers the stub, so a stub left
            # unstriped is given the table's background.
            striping$conditionals <- list(list(
                id = "row_striping",
                selector = list(row_parity = "even", scope = "tbody"),
                style = list(background_color = as.character(color))
            ))
            if (!include_stub && is.null(stub$background_color)) {
                stub$background_color <- as.character(raw("table_background_color"))
            }
        }
    }

    list(
        head_row = id_of(head_row),
        group_label = group_label,
        stub = stub,
        body_row = id_of(body_row),
        summary_row = id_of(fill("summary_row_background_color")),
        grand_summary_row = id_of(fill("grand_summary_row_background_color")),
        striping = striping
    )
}

# A style id for `option_def` (a definition list) overlaid by the style `id`
# already has (tab_style() wins); NULL if both are empty.
.gw_merge_style <- function(styles, option_def, id) {
    if (length(option_def) == 0) return(id)
    def <- if (is.null(id)) option_def else utils::modifyList(option_def, styles$get(id))
    styles$id(def)
}

.gw_cell <- function(content, colspan = 1L, style_id = NULL, is_stub = FALSE,
                     is_placeholder = FALSE, scope = NULL, data_type = NULL,
                     sort_key = NULL) {
    list(
        content = content,
        colspan = as.integer(colspan),
        rowspan = 1L,
        style_id = style_id,
        is_stub = is_stub,
        is_placeholder = is_placeholder,
        scope = scope,
        sort_key = sort_key,
        data_type = data_type
    )
}

.gw_align <- function(align, is_stub = FALSE) {
    if (is_stub) return("left")
    if (length(align) == 0 || is.na(align) || !align %in% c("left", "center", "right")) {
        return("left")
    }
    align
}

# ---- Footnotes ------------------------------------------------------------------

# Location keys for footnotes and styles: "title", "subtitle", "stubhead",
# "columns_columns:<var>", "columns_groups:<spanner id>", "row_groups:<group id>",
# "stub:<row>", "data:<row>:<var>" (rows in display order), "source_notes" and
# "footnotes".
.gw_location_key <- function(locname, grpname, colname, rownum) {
    switch(locname,
        title = "title",
        subtitle = "subtitle",
        stubhead = "stubhead",
        columns_columns = paste0("columns_columns:", colname),
        columns_groups = paste0("columns_groups:", grpname),
        row_groups = paste0("row_groups:", grpname),
        stub = paste0("stub:", rownum),
        data = paste0("data:", rownum, ":", colname),
        source_notes = "source_notes",
        footnotes = "footnotes",
        NULL
    )
}

# Footnote marks by location, and the footer's footnotes in mark order (gt's
# `footnotes_marks` option sets the marks).
.gw_footnote_marks <- function(built, styles) {
    fns <- built[["_footnotes"]]
    if (is.null(fns) || nrow(fns) == 0) {
        return(list(footnotes = list(), lookup = function(key) NULL))
    }
    marks_opt <- utils::getFromNamespace("dt_options_get_value", "gt")(
        data = built, option = "footnotes_marks"
    )
    process_marks <- utils::getFromNamespace("process_footnote_marks", "gt")
    mark_of <- function(id) process_marks(x = id, marks = marks_opt)

    fs_id <- as.integer(fns$fs_id)
    ids <- sort(unique(fs_id))
    footnotes <- lapply(ids, function(id) {
        list(
            id = paste0("fn", id),
            mark = mark_of(id),
            content = .gw_nodes(fns$footnotes[[which(fs_id == id)[1]]], styles),
            style_id = NULL
        )
    })

    by_key <- list()
    for (i in seq_len(nrow(fns))) {
        key <- .gw_location_key(fns$locname[i], fns$grpname[i], fns$colname[i], fns$rownum[i])
        if (is.null(key)) next
        placement <- if (is.null(fns$placement)) "auto" else fns$placement[i]
        by_key[[key]] <- c(by_key[[key]], list(list(id = fs_id[i], placement = placement)))
    }
    by_key <- lapply(by_key, function(entries) {
        ids <- vapply(entries, function(e) e$id, integer(1))
        keep <- !duplicated(ids)
        entries <- entries[keep][order(ids[keep])]
        list(
            marks = lapply(entries, function(e) list(id = e$id, mark = mark_of(e$id))),
            placement = entries[[1]]$placement
        )
    })
    list(footnotes = footnotes, lookup = function(key) by_key[[key]])
}

# Content with footnote marks: after the text, or before it for "left" placement
# (and for "auto" in right-aligned body cells, as gt does). Several marks on one
# location are separated by commas.
.gw_with_marks <- function(nodes, entry, right_aligned = FALSE) {
    if (is.null(entry)) return(nodes)
    n <- length(entry$marks)
    mark_nodes <- lapply(seq_len(n), function(k) {
        m <- entry$marks[[k]]
        list(
            type = "footnote_mark",
            ref = paste0("fn", m$id),
            mark_text = if (k < n) paste0(m$mark, ",") else m$mark
        )
    })
    left <- identical(entry$placement, "left") ||
        (identical(entry$placement, "auto") && right_aligned)
    if (left) c(mark_nodes, nodes) else c(nodes, mark_nodes)
}

# ---- Styles ---------------------------------------------------------------------

# Style definitions, deduplicated: identical definitions share an ID.
.gw_style_registry <- function() {
    defs <- list()
    keys <- character(0)
    list(
        id = function(def) {
            def <- def[order(names(def))]
            key <- paste(deparse(def), collapse = "")
            hit <- match(key, keys)
            if (!is.na(hit)) return(names(defs)[hit])
            id <- paste0("gw_", length(defs) + 1L)
            defs[[id]] <<- def
            keys <<- c(keys, key)
            id
        },
        get = function(id) defs[[id]],
        defs = function() if (length(defs) == 0) setNames(list(), character(0)) else defs
    )
}

# `tab_style()` styles by location key, several styles on one location merged.
.gw_cell_styles <- function(built, styles) {
    st <- built[["_styles"]]
    by_key <- list()
    if (!is.null(st) && nrow(st) > 0) {
        for (i in seq_len(nrow(st))) {
            key <- .gw_location_key(st$locname[i], st$grpname[i], st$colname[i], st$rownum[i])
            if (is.null(key)) next
            def <- .gw_style_def(st$styles[[i]])
            if (length(def) == 0) next
            base <- if (is.null(by_key[[key]])) list() else by_key[[key]]
            by_key[[key]] <- utils::modifyList(base, def)
        }
    }
    ids <- lapply(by_key, styles$id)
    list(lookup = function(key) ids[[key]])
}

# A gt `cell_styles` list as an IR style definition.
.gw_style_def <- function(cell_styles) {
    text_map <- c(
        font = "font_family", size = "font_size", color = "color",
        weight = "font_weight", style = "font_style", align = "text_align",
        v_align = "vertical_align", transform = "text_transform",
        decorate = "text_decoration", whitespace = "white_space", indent = "indent"
    )
    def <- list()
    for (s in cell_styles) {
        if (inherits(s, "cell_text")) {
            for (name in intersect(names(s), names(text_map))) {
                value <- s[[name]]
                if (is.null(value) || length(value) == 0) next
                value <- as.character(value[1])
                if (name == "font") value <- gsub("^['\"]|['\"]$", "", value)
                def[[text_map[[name]]]] <- value
            }
        } else if (inherits(s, "cell_fill")) {
            def$background_color <- as.character(s$color)
        } else if (inherits(s, "cell_border")) {
            if (is.null(def$border)) def$border <- list()
            def$border[[s$side]] <- list(width = s$width, style = s$style, color = s$color)
        }
    }
    def
}

# ---- Values ---------------------------------------------------------------------

# A numeric cell's value as displayed: the number gt shows, so a spreadsheet
# holds what the table states (0.7, not 0.718317...). Cells whose display isn't
# a plain number (`<0.001`, `46 (37, 60)`) have no typed value.
.gw_typed_value <- function(raw, text) {
    if (is.null(raw) || length(raw) == 0 || is.na(raw) || !nzchar(text)) return(NULL)
    if (is.numeric(raw)) {
        # U+2212 MINUS SIGN, matched as UTF-8 bytes so the locale does not matter.
        shown <- gsub("\xe2\x88\x92", "-", text, fixed = TRUE, useBytes = TRUE)
        shown <- gsub(",", "", shown, fixed = TRUE)
        if (!grepl("^-?[0-9]+(\\.[0-9]+)?([eE][-+]?[0-9]+)?$", shown)) return(NULL)
        type <- if (is.integer(raw) && !grepl("[.eE]", shown)) "integer" else "number"
        return(list(type = type, value = as.numeric(shown)))
    }
    if (is.logical(raw)) return(list(type = "boolean", value = raw))
    if (inherits(raw, "Date")) return(list(type = "date", value = format(raw, "%Y-%m-%d")))
    list(type = "string", value = text)
}

.gw_dtype <- function(raw) {
    if (is.null(raw) || length(raw) == 0) return(NULL)
    if (is.integer(raw)) return("integer")
    if (is.numeric(raw)) return("number")
    if (is.logical(raw)) return("boolean")
    if (inherits(raw, "Date")) return("date")
    if (inherits(raw, "POSIXt")) return("datetime")
    "string"
}
