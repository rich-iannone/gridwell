#' Convert a gt table object to gridwell IR JSON
#'
#' Cell values come from gt's own formatting step (as they appear in gt's
#' rendered output, not the raw data). `md()` text becomes bold and italic
#' runs, spanners become spanner rows, footnote marks are placed in the cells
#' they annotate, and `tab_style()` styles become IR styles.
#'
#' The converter is moving into gt, as `gt::as_ir()`. With `emitter = "auto"`
#' it is used when the installed gt has it, and gridwell's own copy otherwise,
#' or when gt's fails or gives an IR version this gridwell can't read (with a
#' warning saying why).
#'
#' @param gt_obj A `gt_tbl` object from the gt package.
#' @param emitter Which converter to use: `"auto"` (the default), `"upstream"`
#'   (gt's own, an error if it can't be used) or `"gridwell"` (gridwell's).
#' @return A JSON string in gridwell IR format.
#' @export
gt_to_ir <- function(gt_obj, emitter = c("auto", "upstream", "gridwell")) {
    upstream <- .gw_upstream_ir(gt_obj, match.arg(emitter))
    if (!is.null(upstream)) {
        return(structure(upstream$json, class = "json"))
    }
    ir <- .gw_build_ir(gt_obj)
    jsonlite::toJSON(ir, auto_unbox = TRUE, null = "null", pretty = TRUE, digits = NA)
}

#' Convert a gt table object to a gridwell IR list
#'
#' @inheritParams gt_to_ir
#' @return A list representing the gridwell IR.
#' @export
gt_to_ir_list <- function(gt_obj, emitter = c("auto", "upstream", "gridwell")) {
    upstream <- .gw_upstream_ir(gt_obj, match.arg(emitter))
    if (!is.null(upstream)) {
        return(upstream$ir)
    }
    .gw_build_ir(gt_obj)
}

# The IR major version this gridwell reads.
.gw_ir_major <- "1"

# gt's own IR for `gt_obj`, as `list(json, ir)`, or NULL to use gridwell's own.
.gw_upstream_ir <- function(gt_obj, emitter) {
    if (!inherits(gt_obj, "gt_tbl")) {
        stop("`gt_obj` must be a gt table (a `gt_tbl` object).", call. = FALSE)
    }
    if (!requireNamespace("gt", quietly = TRUE)) {
        stop("Converting a gt table needs the gt package.", call. = FALSE)
    }
    if (emitter == "gridwell") {
        return(NULL)
    }
    as_ir <- .gw_upstream_emitter()
    if (is.null(as_ir)) {
        if (emitter == "upstream") {
            stop("gt ", utils::packageVersion("gt"), " has no `as_ir()`; use ",
                 "`emitter = \"gridwell\"`.", call. = FALSE)
        }
        return(NULL)
    }
    if (emitter == "upstream") {
        return(.gw_checked_ir(as_ir(gt_obj)))
    }
    tryCatch(.gw_checked_ir(as_ir(gt_obj)), error = function(e) {
        warning(conditionMessage(e), "; converted with gridwell's own emitter instead",
                call. = FALSE)
        NULL
    })
}

# gt's `as_ir()`, or NULL if the installed gt has none.
.gw_upstream_emitter <- function(pkg = "gt", name = "as_ir") {
    if (!requireNamespace(pkg, quietly = TRUE) || !name %in% getNamespaceExports(pkg)) {
        return(NULL)
    }
    f <- getExportedValue(pkg, name)
    if (is.function(f)) f else NULL
}

# `list(json, ir)` for what `gt::as_ir()` gave, if it's IR this gridwell reads.
.gw_checked_ir <- function(json) {
    if (!is.character(json) || length(json) != 1L || is.na(json)) {
        stop("gt::as_ir() gave ", .gw_show(json), ", not a JSON string", call. = FALSE)
    }
    # parse_json(), not fromJSON(): it never reads a file or URL the string names.
    ir <- tryCatch(
        jsonlite::parse_json(json, simplifyVector = FALSE),
        error = function(e) {
            stop("gt::as_ir() gave JSON that doesn't parse (", trimws(conditionMessage(e)), ")",
                 call. = FALSE)
        }
    )
    version <- if (is.list(ir)) ir[["ir_version"]]
    if (!is.character(version) || length(version) != 1L ||
        !grepl("^[0-9]+\\.[0-9]+\\z", version, perl = TRUE)) {
        stop("gt::as_ir() gave an ir_version of ", .gw_show(version), ", not MAJOR.MINOR",
             call. = FALSE)
    }
    major <- sub("^0*([0-9]+)\\..*", "\\1", version)
    if (major != .gw_ir_major) {
        stop("gt::as_ir() gives IR ", version, ", and this gridwell reads IR ", .gw_ir_major,
             ".x (a newer gridwell may read it)", call. = FALSE)
    }
    list(json = json, ir = ir)
}

.gw_show <- function(x) {
    if (is.null(x)) {
        "null"
    } else if (is.character(x) && length(x) == 1L && !is.na(x)) {
        encodeString(x, quote = "\"")
    } else if (length(x) == 1L && is.atomic(x)) {
        paste0("a ", class(x)[1], " (", format(x), ")")
    } else {
        paste0("a ", class(x)[1], " of length ", length(x))
    }
}

# gridwell's own emitter: the IR of `gt_obj`, as a list.
.gw_build_ir <- function(gt_obj) {
    built <- .gw_build_gt(gt_obj)
    styles <- .gw_style_registry()

    data <- built[["_data"]]
    body <- built[["_body"]]
    boxhead <- built[["_boxhead"]]
    stub_df <- built[["_stub_df"]]
    # The build turns the heading, column labels and source notes into plain text;
    # the unbuilt table still has their markdown.
    heading <- gt_obj[["_heading"]]

    # Visible columns: the stub columns (if any) first, then the default columns.
    summaries <- .gw_summaries(built)
    stub <- .gw_stub_layout(built, summaries)
    n_stub <- length(stub$vars)
    all_vars <- c(stub$vars, boxhead$var[boxhead$type == "default"])
    opts <- .gw_option_styles(built, styles)

    marks <- .gw_footnote_marks(built, styles)
    cell_styles <- .gw_cell_styles(built, styles)

    column_spec <- lapply(seq_along(all_vars), function(j) {
        var <- all_vars[j]
        idx <- which(boxhead$var == var)
        width <- if (length(idx) > 0) boxhead$column_width[[idx]] else NULL
        width <- if (is.null(width) || !nzchar(width[1])) "auto" else as.character(width[1])
        list(
            id = var,
            align = .gw_align(
                if (length(idx) > 0) boxhead$column_align[idx] else "left",
                is_stub = j <= n_stub
            ),
            align_char = NULL,
            width = width,
            min_width = NULL,
            max_width = NULL,
            style_id = NULL,
            hidden = FALSE,
            label = .gw_plain(
                if (j <= n_stub) .gw_stub_label(gt_obj, var, stub) else .gw_column_label(gt_obj, var)
            )
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
        cells = c(
            .gw_stubhead_cells(gt_obj, stub, styles, marks, cell_styles),
            lapply(all_vars[seq_along(all_vars) > n_stub], function(var) {
                key <- paste0("columns_columns:", var)
                .gw_cell(
                    .gw_with_marks(
                        .gw_nodes(.gw_column_label(gt_obj, var), styles),
                        marks$lookup(key)
                    ),
                    style_id = cell_styles$lookup(key),
                    scope = "col"
                )
            })
        )
    )
    thead_rows <- c(
        .gw_spanner_rows(built, all_vars, n_stub, styles, marks, cell_styles),
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

    # Body rows, in gt's display order. With a row group column, a group's label is
    # the cell in that column of its first row, spanning its data rows (the IR has
    # no span from a group's rows into its summary rows).
    group_cells <- .gw_group_cells(stub_df, stub, styles, marks, cell_styles, opts)
    spans <- .gw_stub_spans(data, stub_df, stub$rows)
    n_group <- length(stub$group)
    rows <- lapply(seq_len(nrow(body)), function(i) {
        orig <- stub_df$rownum_i[i]
        lead <- if (n_group > 0) {
            cell <- group_cells$at[[as.character(i)]]
            list(if (is.null(cell)) .gw_cell(list(), is_stub = TRUE, is_placeholder = TRUE) else cell)
        }
        cells <- lapply(seq(n_group + 1L, length.out = length(all_vars) - n_group), function(j) {
            var <- all_vars[j]
            is_stub_cell <- j <= n_stub
            rowspan <- 1L
            if (is_stub_cell) {
                rowspan <- spans[i, j - n_group]
                if (rowspan == 0L) {
                    return(.gw_cell(list(), is_stub = TRUE, is_placeholder = TRUE))
                }
                # A stub cell has the styles and footnotes for its column in any row
                # it spans, and (the primary column's) those for no column in its row.
                keys <- c(
                    if (identical(var, stub$primary)) paste0("stub:", i),
                    paste0("stub:", seq(i, length.out = rowspan), ":", var)
                )
            } else {
                keys <- paste0("data:", i, ":", var)
            }
            right <- !is_stub_cell &&
                identical(boxhead$column_align[boxhead$var == var], "right")
            shown <- body[[var]][i]
            text <- .gw_body_text(shown)
            raw <- if (var %in% names(data)) data[[var]][orig] else NULL
            cell_style <- cell_styles$lookup(keys)
            if (is_stub_cell) cell_style <- .gw_merge_style(styles, opts$stub, cell_style)
            cell <- .gw_cell(
                .gw_with_marks(.gw_text_nodes(text), marks$lookup(keys), right_aligned = right),
                rowspan = rowspan,
                style_id = cell_style,
                is_stub = is_stub_cell,
                # A stub cell heads its row, or the rows it spans (as gt has it).
                scope = if (is_stub_cell) (if (rowspan > 1L) "rowgroup" else "row")
            )
            typed <- if (is_stub_cell) NULL else .gw_typed_value(raw, text)
            if (!is.null(typed)) cell$typed_value <- typed
            cell
        })
        list(role = NULL, style_id = opts$body_row, cells = c(lead, cells))
    })

    # Before the styles are listed below: the body registers styles too.
    tbody <- .gw_tbody(
        built, rows, styles, marks, cell_styles, summaries, all_vars, stub, opts,
        group_cells$style
    )

    # Footer: footnotes (in mark order) and source notes.
    footer <- list(footnotes = marks$footnotes, source_notes = list())
    source_notes <- gt_obj[["_source_notes"]]
    if (length(source_notes) > 0) {
        footer$source_notes <- lapply(source_notes, function(sn) {
            list(content = .gw_nodes(sn, styles), style_id = cell_styles$lookup("source_notes"))
        })
    }

    config <- list(
        table_cols = length(all_vars),
        header_rows = length(thead_rows),
        body_rows = length(rows),
        stub_cols = n_stub,
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
    )
    table_id <- .gw_table_id(built)
    if (!is.null(table_id)) config$id <- table_id
    has_heading <- !is.null(header$title) || !is.null(header$subtitle)
    has_footer <- length(footer$footnotes) > 0 || length(footer$source_notes) > 0
    rules <- .gw_table_rules(built, has_heading, has_footer)
    if (length(rules) > 0) config$rules <- rules
    # A locale set on the table (`gt(locale = ...)`) is its language; the
    # direction follows from the language, since the IR doesn't infer it.
    locale <- gt_obj[["_locale"]]$locale
    if (is.character(locale) && length(locale) == 1 && nzchar(locale)) {
        lang <- gsub("_", "-", locale, fixed = TRUE)
        config$locale <- lang
        config$lang <- lang
        if (.gw_is_rtl(lang)) config$dir <- "rtl"
    }

    list(
        ir_version = "1.0",
        config = config,
        styles = list(
            defs = styles$defs(),
            compositions = stats::setNames(list(), character(0)),
            conditionals = opts$striping$conditionals
        ),
        header = header,
        column_spec = column_spec,
        table = list(
            thead = list(rows = thead_rows),
            tbody = tbody
        ),
        footer = footer,
        extensions = stats::setNames(list(), character(0))
    )
}

# Scripts written right to left, and languages written right to left when the
# tag names no script (as in gridwell_core::lang).
.gw_rtl_scripts <- c(
    "Adlm", "Arab", "Aran", "Hebr", "Mand", "Mend", "Nkoo", "Rohg", "Samr", "Syrc", "Thaa",
    "Yezi"
)
.gw_rtl_languages <- c(
    "ar", "arc", "bal", "bqi", "ckb", "dv", "fa", "glk", "he", "iw", "ji", "ks", "lrc",
    "mzn", "nqo", "pnb", "prs", "ps", "sd", "sdh", "syr", "ug", "ur", "yi"
)

# Whether a language tag's text is written right to left.
.gw_is_rtl <- function(lang) {
    subtags <- strsplit(lang, "-", fixed = TRUE)[[1]]
    if (length(subtags) > 1 && grepl("^[A-Za-z]{4}$", subtags[2])) {
        script <- paste0(toupper(substr(subtags[2], 1, 1)), tolower(substring(subtags[2], 2)))
        return(script %in% .gw_rtl_scripts)
    }
    tolower(subtags[1]) %in% .gw_rtl_languages
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

# A column's label, from the unbuilt table.
.gw_column_label <- function(gt_obj, var) {
    boxhead <- gt_obj[["_boxhead"]]
    label <- boxhead$column_label[[which(boxhead$var == var)]]
    if (is.null(label)) var else label
}

# The stub columns, as gt lays them out (its `get_stub_layout()`): the row group
# column (`row_group_as_column = TRUE`), then the stub columns, outermost first (a
# hierarchical stub, `rowname_col = c(...)`, has several), the primary last; as
# `list(group, rows, vars, primary)`, `vars` all of them.
#
# Summary rows need a stub for their labels: gt adds one when the table has none
# (empty in the data rows), and so does the IR. With a row group column and no
# stub, it adds none: the summary labels are in the group column.
.gw_stub_layout <- function(built, summaries) {
    boxhead <- built[["_boxhead"]]
    stub_df <- built[["_stub_df"]]
    layout <- utils::getFromNamespace("get_stub_layout", "gt")(data = built)
    has_groups <- length(built[["_row_groups"]]) > 0 && !all(is.na(stub_df$group_id))
    group <- NULL
    if ("group_label" %in% layout && has_groups) {
        group <- boxhead$var[boxhead$type == "row_group"]
        group <- if (length(group) > 0) group[1] else .gw_group_stub
    }
    # (gt's build lists the stub it adds for summary rows in the boxhead, even when
    # the layout has no place for it.)
    rows <- if ("rowname" %in% layout) boxhead$var[boxhead$type == "stub"] else character(0)
    if (length(rows) == 0 && ("rowname" %in% layout || (is.null(group) && length(summaries) > 0))) {
        rows <- .gw_summary_stub
    }
    list(
        group = group,
        rows = rows,
        vars = c(group, rows),
        primary = if (length(rows) > 0) rows[length(rows)]
    )
}

# The row group column's id when gt has no column for the groups.
.gw_group_stub <- "::group::"

# The stubhead's labels: one across the stub ("" when there is none), or one per
# stub column when `tab_stubhead()` was given a label for each.
.gw_stubhead_labels <- function(gt_obj, n_rows) {
    label <- gt_obj[["_stubhead"]]$label
    if (is.null(label) || length(label) == 0) return(list(""))
    # Each label keeps the class (`md()`, `html()`) of the vector.
    one <- if (is.list(label)) {
        function(k) label[[k]]
    } else {
        function(k) {
            x <- label[k]
            oldClass(x) <- oldClass(label)
            x
        }
    }
    if (length(label) > 1 && length(label) == n_rows) return(lapply(seq_along(label), one))
    list(one(1L))
}

# A stub column's label: the stubhead's for the primary column (or a column's own,
# given one for each), else the column's.
.gw_stub_label <- function(gt_obj, var, stub) {
    labels <- .gw_stubhead_labels(gt_obj, length(stub$rows))
    k <- match(var, stub$rows)
    if (length(labels) > 1 && !is.na(k)) return(labels[[k]])
    if (identical(var, stub$primary)) return(labels[[1]])
    if (var %in% gt_obj[["_boxhead"]]$var) .gw_column_label(gt_obj, var) else ""
}

# The column label row's cells for the stub: the stubhead label across the stub
# columns and left-aligned, as gt shows it; given a label for each stub column,
# each has its own (and a row group column a blank one), all with the footnote
# marks, as gt draws them.
.gw_stubhead_cells <- function(gt_obj, stub, styles, marks, cell_styles) {
    n_stub <- length(stub$vars)
    if (n_stub == 0) return(list())
    labels <- .gw_stubhead_labels(gt_obj, length(stub$rows))
    style_id <- cell_styles$lookup("stubhead")
    label_cell <- function(label, ...) {
        .gw_cell(.gw_with_marks(.gw_nodes(label, styles), marks$lookup("stubhead")), ...)
    }
    if (length(labels) == 1) {
        # Left-aligned across the stub, where a cell across columns is centered.
        across <- if (n_stub > 1) .gw_merge_style(styles, list(text_align = "left"), style_id)
        return(c(
            list(label_cell(
                labels[[1]],
                colspan = n_stub,
                style_id = if (n_stub > 1) across else style_id,
                scope = if (n_stub > 1) "colgroup" else "col"
            )),
            .gw_placeholders(n_stub - 1L)
        ))
    }
    c(
        if (!is.null(stub$group)) list(.gw_cell(list(), style_id = style_id, scope = "col")),
        lapply(labels, label_cell, style_id = style_id, scope = "col")
    )
}

# `n` cells covered by a span.
.gw_placeholders <- function(n, is_stub = FALSE) {
    lapply(seq_len(n), function(k) .gw_cell(list(), is_stub = is_stub, is_placeholder = TRUE))
}

# For each data row as shown, each of the stub columns `vars`' rowspan: 0 for a
# cell covered by one above. In a hierarchical stub, as gt merges it: an outer
# column's cells merge across adjacent rows of a group with the same value in
# that column and every column to its left. The primary column's never do.
.gw_stub_spans <- function(data, stub_df, vars) {
    n <- nrow(stub_df)
    spans <- matrix(1L, nrow = n, ncol = length(vars))
    if (length(vars) < 2 || n == 0) return(spans)
    group <- stub_df$group_id
    # The rows' values as shown (gt compares the data's rows by their place in the
    # display, wrongly when groups reorder them).
    values <- lapply(vars, function(var) data[[var]][stub_df$rownum_i])
    same <- function(a, b, level) {
        identical(group[a], group[b]) &&
            all(vapply(seq_len(level), function(l) identical(values[[l]][a], values[[l]][b]),
                       logical(1)))
    }
    for (level in seq_len(length(vars) - 1L)) {
        start <- 1L
        for (j in seq(2L, n + 1L)) {
            if (j <= n && same(j, start, level)) next
            spans[start, level] <- j - start
            if (j - start > 1L) spans[seq(start + 1L, j - 1L), level] <- 0L
            start <- j
        }
    }
    spans
}

# A row group's label as gt shows it.
.gw_group_label <- function(stub_df, grp_id) {
    idx <- which(stub_df$group_id == grp_id)
    label <- if (length(idx) > 0) stub_df$group_label[[idx[1]]]
    if (is.null(label)) grp_id else label
}

# With a row group column, each group's label cell, by the data row (as shown) it
# starts in (`at`), and its style id by group id (`style`). It spans the group's
# data rows; its styles are the group label's, under the table options for the
# row group column (not the group labels' or the stub's), and it isn't striped,
# as gt draws it.
.gw_group_cells <- function(stub_df, stub, styles, marks, cell_styles, opts) {
    out <- list(at = list(), style = list())
    if (is.null(stub$group) || nrow(stub_df) == 0) return(out)
    ids <- stub_df$group_id
    runs <- rle(ifelse(is.na(ids), "\001", ids))
    starts <- cumsum(c(1L, runs$lengths))[seq_along(runs$lengths)]
    for (k in seq_along(starts)) {
        i <- starts[k]
        grp_id <- ids[i]
        if (is.na(grp_id)) next
        key <- paste0("row_groups:", grp_id)
        style_id <- .gw_merge_style(styles, opts$group_cell(i), cell_styles$lookup(key))
        out$at[[as.character(i)]] <- .gw_cell(
            .gw_with_marks(.gw_nodes(.gw_group_label(stub_df, grp_id), styles), marks$lookup(key)),
            rowspan = runs$lengths[k],
            style_id = style_id,
            is_stub = TRUE,
            scope = "rowgroup"
        )
        if (is.null(out$style[[grp_id]])) out$style[grp_id] <- list(style_id)
    }
    out
}

# One spanner row per spanner level, the top level first, as gt draws them. A
# spanner covers a run of adjacent columns, followed by placeholder cells. In the
# lowest spanner row each stub column is a blank cell of its own (the stubhead
# reaches up to it in gt's HTML); in the rows above, the stub columns share one.
.gw_spanner_rows <- function(built, all_vars, n_stub, styles, marks, cell_styles) {
    spanners <- built[["_spanners"]]
    if (is.null(spanners) || nrow(spanners) == 0) return(list())
    levels <- sort(unique(spanners$spanner_level), decreasing = TRUE)
    lapply(levels, function(level) {
        at_level <- spanners[spanners$spanner_level == level, , drop = FALSE]
        owner <- vapply(all_vars, function(var) {
            hit <- which(vapply(at_level$vars, function(v) var %in% v, logical(1)))
            if (length(hit) > 0) at_level$spanner_id[hit[1]] else NA_character_
        }, character(1))
        cells <- list()
        if (n_stub > 0 && level == min(levels)) {
            cells <- lapply(seq_len(n_stub), function(k) .gw_cell(list(), scope = "col"))
        } else if (n_stub > 0) {
            cells <- c(
                list(.gw_cell(list(), colspan = n_stub, scope = if (n_stub > 1) "colgroup" else "col")),
                .gw_placeholders(n_stub - 1L)
            )
        }
        j <- n_stub + 1L
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

# The body's row groups, then the grand summary (as a group of summary rows alone;
# its rows at the top are a group before the others).
# With a row group column (its cells' style ids by group id in `group_styles`),
# the groups have no labels of their own.
.gw_tbody <- function(built, rows, styles, marks, cell_styles, summaries, all_vars, stub,
                      opts, group_styles) {
    stub_df <- built[["_stub_df"]]
    row_groups <- built[["_row_groups"]]
    n_stub <- length(stub$vars)
    data_vars <- all_vars[seq_along(all_vars) > n_stub]
    # The label across the stub. With a row group column, a group's summary label
    # is across the other stub columns, as gt draws it under the group's label;
    # the group column is an empty cell there, styled as the group's label
    # (`group_style`), since the IR has no span from a group's rows into its
    # summary rows. A grand summary label is across the whole stub.
    summary_rows <- function(id, role, row_style, group_style = NULL, side = NULL) {
        df <- summaries[[id]]
        if (is.null(df)) return(list())
        if (!is.null(side)) df <- df[.gw_summary_sides(df) == side, , drop = FALSE]
        first <- !is.null(stub$group) && length(stub$rows) > 0 && id != .gw_grand_summary_id
        span <- n_stub - first
        lapply(seq_len(nrow(df)), function(i) {
            cells <- c(
                if (first) list(.gw_cell(list(), style_id = group_style, is_stub = TRUE)),
                list(.gw_cell(
                    .gw_text_nodes(.gw_summary_text(df[["::rowname::"]][i])),
                    colspan = span,
                    is_stub = TRUE,
                    scope = "row"
                )),
                .gw_placeholders(span - 1L, is_stub = TRUE),
                lapply(data_vars, function(var) {
                    .gw_cell(.gw_text_nodes(.gw_summary_text(df[[var]][i])))
                })
            )
            list(role = role, style_id = row_style, cells = cells)
        })
    }
    # The grand summary rows at the top are a group above the others, the rest a
    # group after them.
    grand <- function(side) {
        found <- summary_rows(
            .gw_grand_summary_id, "grand_summary_row", opts$grand_summary_row, side = side
        )
        if (length(found) == 0) return(NULL)
        group <- list(group_id = NULL, label = NULL, rows = list(), summary_rows = found)
        if (side == "top") group$summary_placement <- "top"
        list(group)
    }
    grand_top <- grand("top")
    grand_group <- grand("bottom")
    if (length(row_groups) == 0 || all(is.na(stub_df$group_id))) {
        return(c(
            grand_top,
            list(list(group_id = NULL, label = NULL, rows = rows, summary_rows = list())),
            grand_group
        ))
    }
    groups <- lapply(row_groups, function(grp_id) {
        idx <- which(stub_df$group_id == grp_id)
        key <- paste0("row_groups:", grp_id)
        label <- if (is.null(stub$group)) {
            list(
                content = .gw_with_marks(
                    .gw_nodes(.gw_group_label(stub_df, grp_id), styles), marks$lookup(key)
                ),
                style_id = .gw_merge_style(styles, opts$group_label, cell_styles$lookup(key)),
                colspan = NULL
            )
        }
        group <- list(
            group_id = grp_id,
            label = label,
            rows = rows[idx],
            summary_rows = summary_rows(
                grp_id, "summary_row", opts$summary_row, group_styles[[grp_id]]
            )
        )
        # A group has one placement: summary rows on both sides are all after its
        # rows.
        sides <- .gw_summary_sides(summaries[[grp_id]])
        if (length(sides) > 0 && all(sides == "top")) group$summary_placement <- "top"
        group
    })
    c(grand_top, groups, grand_group)
}

# ---- Summary rows -------------------------------------------------------------------

# gt's id for the grand summary in its built summary data, and the stub column the
# IR gets when a table with summaries has no stub of its own.
.gw_grand_summary_id <- "::GRAND_SUMMARY"
.gw_summary_stub <- "::rowname::"

# gt's formatted summary rows, by group id (`.gw_grand_summary_id` for the grand
# summary): data frames with `::rowname::` (the label) and one column per variable,
# holding the text gt displays, and `::side::` (`top` or `bottom`, from `side =`).
.gw_summaries <- function(built) {
    if (length(built[["_summary"]]) == 0) return(list())
    get <- utils::getFromNamespace("dt_summary_df_get", "gt")
    display <- get(data = built)$summary_df_display_list
    if (is.null(display)) return(list())
    lapply(display, as.data.frame, stringsAsFactors = FALSE)
}

# The side of each of a summary data frame's rows (`bottom` when gt has none).
.gw_summary_sides <- function(df) {
    if (is.null(df) || nrow(df) == 0) return(character(0))
    sides <- df[["::side::"]]
    if (is.null(sides)) return(rep("bottom", nrow(df)))
    ifelse(is.na(sides), "bottom", as.character(sides))
}

.gw_summary_text <- function(x) {
    if (is.null(x) || length(x) == 0 || is.na(x)) return("")
    as.character(x)
}

# ---- Table options ------------------------------------------------------------------

# The table's id (`gt(id =)`), when it's one the IR takes (an HTML id: not empty,
# no whitespace).
.gw_table_id <- function(built) {
    options <- built[["_options"]]
    id <- options$value[[match("table_id", options$parameter)]]
    if (is.character(id) && length(id) == 1 && !is.na(id) && nzchar(id) &&
        !grepl("[[:space:]]", id)) {
        id
    }
}

# gt's border options drawn on each of gridwell's rules (`config.rules`), by the
# start of their names (`<name>_style`, `_width`, `_color`), the outer one last.
# Which ones are drawn on the table's top and bottom depends on whether there's a
# heading and a footer: `table_border_top` is above the heading, or the table's top
# without one (and `table_border_bottom` likewise). The same as the Python
# emitter's `_rule_options`.
.gw_rule_options <- function(has_heading, has_footer) {
    list(
        title = if (has_heading) "table_border_top",
        top = c(
            "column_labels_border_top",
            if (has_heading) "heading_border_bottom" else "table_border_top"
        ),
        column_labels = c("table_body_border_top", "column_labels_border_bottom"),
        spanners = "column_labels_border_bottom",
        group = c("row_group_border_bottom", "row_group_border_top"),
        rows = "table_body_hlines",
        stub = "stub_border",
        summary = "summary_row_border",
        grand_summary = "grand_summary_row_border",
        bottom = c("table_body_border_bottom", if (!has_footer) "table_border_bottom"),
        notes = if (has_footer) "table_border_bottom"
    )
}

.gw_rule_styles <- c("none", "hidden", "dotted", "dashed", "solid", "double")

# The rules of the table's border options a user changed, as `config.rules`: each
# option at gt's default is left out, so a rule changes only what the options
# change (and the default look draws the rest). Where two options are drawn on one
# rule, the outer one's changes win. Styles the IR has no rule for (`groove`, ...)
# are left out.
.gw_table_rules <- function(built, has_heading, has_footer) {
    options <- built[["_options"]]
    defaults <- gt::gt(data.frame(x = 1))[["_options"]]
    changed <- function(name) {
        i <- match(name, options$parameter)
        if (is.na(i)) return(NULL)
        v <- options$value[[i]]
        d <- defaults$value[[match(name, defaults$parameter)]]
        if (identical(v, d) || is.null(v) || length(v) == 0 || is.na(v[1])) NULL else v[1]
    }
    drawn <- .gw_rule_options(has_heading, has_footer)
    rules <- list()
    for (rule in names(drawn)) {
        border <- list()
        for (name in drawn[[rule]]) {
            style <- changed(paste0(name, "_style"))
            if (is.character(style) && tolower(style) %in% .gw_rule_styles) {
                border$style <- tolower(style)
            }
            width <- changed(paste0(name, "_width"))
            if (is.numeric(width)) {
                border$width <- paste0(format(width), "px")
            } else if (is.character(width) && nzchar(width)) {
                border$width <- width
            }
            color <- changed(paste0(name, "_color"))
            if (is.character(color) && nzchar(color)) border$color <- color
        }
        if (length(border) > 0) rules[[rule]] <- border
    }
    rules
}

# The table options a user changed (`tab_options()`, `opt_stylize()`,
# `opt_row_striping()`) that the IR can express, as style ids for each part plus
# the striping config. Options at gt's defaults are left out: gridwell's writers
# supply their own default look. The border options are the table's rules
# (`.gw_table_rules`).
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

    # The row group column's cells (`row_group_as_column`), which gt never stripes:
    # one starting in a striped row (the `i`-th shown) is given the table's background.
    stub_group <- c(
        fill("stub_row_group_background_color"), weight("stub_row_group_font_weight")
    )
    stub_striped <- striping$include_stub || length(striping$conditionals) > 0
    group_cell <- function(i) {
        if (stub_striped && i %% 2 == 0 && is.null(stub_group$background_color)) {
            return(c(list(background_color = as.character(raw("table_background_color"))), stub_group))
        }
        stub_group
    }

    list(
        head_row = id_of(head_row),
        group_label = group_label,
        stub = stub,
        group_cell = group_cell,
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
                     is_placeholder = FALSE, scope = NULL, rowspan = 1L) {
    list(
        content = content,
        colspan = as.integer(colspan),
        rowspan = as.integer(rowspan),
        style_id = style_id,
        is_stub = is_stub,
        is_placeholder = is_placeholder,
        scope = scope
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
# "stub:<row>" (a stub's, for no column), "stub:<row>:<var>" (a stub column's, in
# a hierarchical stub), "data:<row>:<var>" (rows in display order), "source_notes"
# and "footnotes".
.gw_location_key <- function(locname, grpname, colname, rownum) {
    stub <- paste0(
        "stub:", rownum, if (length(colname) == 1 && !is.na(colname)) paste0(":", colname)
    )
    switch(locname,
        title = "title",
        subtitle = "subtitle",
        stubhead = "stubhead",
        columns_columns = paste0("columns_columns:", colname),
        columns_groups = paste0("columns_groups:", grpname),
        row_groups = paste0("row_groups:", grpname),
        stub = stub,
        stub_column = stub,
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
    # The marks of one or more locations (`key`, a vector of keys), together.
    lookup <- function(key) {
        entries <- unlist(unname(by_key[intersect(key, names(by_key))]), recursive = FALSE)
        if (length(entries) == 0) return(NULL)
        ids <- vapply(entries, function(e) e$id, integer(1))
        keep <- !duplicated(ids)
        entries <- entries[keep][order(ids[keep])]
        list(
            marks = lapply(entries, function(e) list(id = e$id, mark = mark_of(e$id))),
            placement = entries[[1]]$placement
        )
    }
    list(footnotes = footnotes, lookup = lookup)
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
        defs = function() if (length(defs) == 0) stats::setNames(list(), character(0)) else defs
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
    # The style of one or more locations (`key`, a vector of keys), merged in order.
    lookup <- function(key) {
        key <- intersect(key, names(by_key))
        if (length(key) == 0) return(NULL)
        if (length(key) == 1) return(ids[[key]])
        styles$id(Reduce(utils::modifyList, by_key[key]))
    }
    list(lookup = lookup)
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

# A cell's typed value. A number is the number displayed, so a spreadsheet holds
# what the table states (0.7, not 0.718317...; 0.451 for `45.1%`; 1234.5 for
# `1.234,50`), read by `.gw_shown_number()`; a display that isn't one number
# (`<0.001`, `46 (37, 60)`) has none. Dates and times are ISO 8601 strings,
# without an offset (a spreadsheet has no time zone: the value is the wall-clock
# time gt shows). Anything else is a string, the text shown.
.gw_typed_value <- function(raw, text) {
    if (is.null(raw) || length(raw) == 0 || is.na(raw) || !nzchar(text)) return(NULL)
    if (is.numeric(raw)) {
        found <- .gw_shown_number(text, raw)
        if (is.null(found)) return(NULL)
        type <- if (is.integer(raw) && found$plain_int) "integer" else "number"
        # (`+ 0`: a `-0` is zero.)
        return(list(type = type, value = as.numeric(paste0(found$digits, "e", found$power)) + 0))
    }
    if (is.logical(raw)) return(list(type = "boolean", value = raw))
    if (inherits(raw, "Date")) return(list(type = "date", value = format(raw, "%Y-%m-%d")))
    if (inherits(raw, "POSIXt")) {
        raw <- as.POSIXct(raw)
        tz <- attr(raw, "tzone")
        tz <- if (is.null(tz)) "" else tz[1]
        return(list(type = "datetime", value = .gw_clock(raw, "%Y-%m-%dT%H:%M:%S", function(s) {
            .POSIXct(s, tz = tz)
        })))
    }
    if (.gw_is_clock_time(raw)) {
        return(list(type = "time", value = .gw_clock(raw, "%H:%M:%S", function(s) {
            .POSIXct(s, tz = "UTC")
        })))
    }
    list(type = "string", value = text)
}

# A time of day or date-time as `format` writes it (in its own time zone), with
# its fraction of a second to the microsecond, if it has one (as Python's
# `isoformat()` writes it). `as_time` makes a time of whole seconds.
.gw_clock <- function(x, format, as_time) {
    secs <- as.numeric(x)
    whole <- floor(round(secs, 6))
    micro <- min(max(round((secs - whole) * 1e6), 0), 999999)
    stamp <- format(as_time(whole), format)
    if (micro > 0) stamp <- sprintf("%s.%06d", stamp, as.integer(micro))
    stamp
}

# An `hms` time that is a time of day (not a duration of a day or more).
.gw_is_clock_time <- function(raw) {
    inherits(raw, "hms") && as.numeric(raw) >= 0 && as.numeric(raw) < 86400
}

# Characters a display may separate thousands with ("." and "," also mark
# decimals).
.gw_group_marks <- c(".", ",", "'", "\u2019", " ", "\u00a0", "\u202f", "\u2009")
# A number as displayed: text before it (a sign, a currency), digits and marks,
# an exponent (`x 10^-4`, superscripts, `e-04`), and text after it (a unit, a `%`).
.gw_number_pattern <- paste0(
    "^([^0-9]*?)",
    "([0-9](?:[0-9", paste(.gw_group_marks, collapse = ""), "]*[0-9])?)",
    "(?:[\\s\u00a0\u202f\u2009]*\u00d7[\\s\u00a0\u202f\u2009]*10",
    "(?:\\^([-\u2212+]?[0-9]+)|([\u207b\u207a]?[\u2070\u00b9\u00b2\u00b3\u2074-\u2079]+))",
    "|[eE]([-\u2212+]?[0-9]+))?",
    "([^0-9]*)$"
)
.gw_signs <- c("-", "\u2212", "+")
# What a mark after the number scales it by (a power of ten).
.gw_per <- c(-2, -3, -4)
names(.gw_per) <- c("%", "\u2030", "\u2031")
.gw_compact <- c(K = 3, M = 6, B = 9, T = 12)

# The characters of `s`.
.gw_chars <- function(s) strsplit(s, "", fixed = TRUE)[[1]]

.gw_all_digits <- function(s) grepl("^[0-9]+$", s)

# An integer part with no leading zeros (`007`).
.gw_plain_int <- function(int) !(nchar(int) > 1 && startsWith(int, "0"))

# The digits of an integer written with `mark` between groups of three (or, as
# India writes them, of two then a last three), if it is.
.gw_grouped <- function(int, mark) {
    if (!nzchar(int) || endsWith(int, mark)) return(NULL)
    groups <- strsplit(int, mark, fixed = TRUE)[[1]]
    first <- groups[1]
    rest <- groups[-1]
    if (length(rest) == 0 || !all(.gw_all_digits(groups))) return(NULL)
    if (nchar(first) > 3 || first == "0") return(NULL)
    n <- nchar(rest)
    threes <- all(n == 3)
    indian <- nchar(first) <= 2 && n[length(n)] == 3 && all(n[-length(n)] == 2)
    if (threes || indian) paste(groups, collapse = "") else NULL
}

# The ways to read a display's digits and marks as a number, each its digits and
# how many of them are decimals: with no marks, with one mark as either the
# decimal mark or the thousands separator, or with two, the last the decimal
# mark. An integer part has no leading zeros (`007`).
.gw_readings <- function(core) {
    chars <- .gw_chars(core)
    is_mark <- !grepl("[0-9]", chars)
    marks <- unique(chars[is_mark])
    decimal <- function(mark, group) {
        if (!mark %in% c(".", ",")) return(NULL)
        at <- max(gregexpr(mark, core, fixed = TRUE)[[1]])
        if (at < 1) return(NULL)
        int <- substr(core, 1, at - 1)
        frac <- substr(core, at + 1, nchar(core))
        if (!.gw_all_digits(frac)) return(NULL)
        whole <- if (is.null(group)) (if (.gw_all_digits(int)) int) else .gw_grouped(int, group)
        if (is.null(whole) || !.gw_plain_int(whole)) return(NULL)
        list(digits = paste0(whole, frac), decimals = nchar(frac))
    }
    if (length(marks) == 0) {
        return(if (.gw_plain_int(core)) list(list(digits = core, decimals = 0)) else list())
    }
    if (length(marks) == 1) {
        whole <- .gw_grouped(core, marks)
        found <- list(
            decimal(marks, NULL),
            if (!is.null(whole) && .gw_plain_int(whole)) list(digits = whole, decimals = 0)
        )
        return(Filter(Negate(is.null), found))
    }
    if (length(marks) == 2) {
        # The decimal mark is the last mark, and comes once.
        last <- utils::tail(chars[is_mark], 1)
        found <- decimal(last, setdiff(marks, last))
        return(if (is.null(found)) list() else list(found))
    }
    list()
}

# Text that can stand beside a number in its display: letters (a unit, a
# currency code), a currency symbol, spaces, `'`, U+2019, a degree sign, a `.`
# beside letters (`kr.`), and after the number one `%`, U+2030 or U+2031. Not `<`,
# `~`, a plus-minus sign, `/`, brackets or other digits: those say something else
# about the number.
.gw_affix <- function(s, after) {
    if (!nzchar(s)) return(TRUE)
    per <- sum(.gw_chars(s) %in% names(.gw_per))
    letters <- grepl("\\p{L}", s, perl = TRUE)
    grepl("^[\\p{L}\\p{Z}\\s\\p{Sc}'\u2019\u00b0.%\u2030\u2031]*$", s, perl = TRUE) &&
        per <= (if (after) 1 else 0) &&
        (letters || !grepl(".", s, fixed = TRUE))
}

# The number a cell's display shows, as the digits (signed) and power of ten of
# its value, and whether it's a plain integer (no decimals, exponent or scale).
#
# The display is read in any marks (a locale's `1.234,50`, `1 234,5`), with a
# sign or parentheses, a currency or unit around it, `%` (a hundredth), U+2030,
# an exponent (`1.23 x 10^-4`) or a `K`, `M`, `B` or `T` (thousands, millions,
# ...); the reading the data's value `raw` rounds to wins (so `1,234` is a
# thousand and more, or a little more than one). With none, a display only one
# reading fits is still read as it shows (a value scaled before being shown:
# `scale_by =`), and one that fits several isn't. As the Python emitter reads it.
.gw_shown_number <- function(text, raw) {
    shown <- trimws(enc2utf8(text))
    parens <- startsWith(shown, "(") && endsWith(shown, ")")
    if (parens) shown <- substr(shown, 2, nchar(shown) - 1)
    m <- regmatches(shown, regexec(.gw_number_pattern, shown, perl = TRUE))[[1]]
    if (length(m) == 0) return(NULL)
    before <- m[2]
    after <- m[7]
    before_chars <- .gw_chars(before)
    signs <- before_chars[before_chars %in% .gw_signs]
    if (length(signs) > 1 || (parens && length(signs) > 0)) return(NULL)
    unsigned <- paste(before_chars[!before_chars %in% .gw_signs], collapse = "")
    if (!.gw_affix(unsigned, FALSE) || !.gw_affix(after, TRUE)) return(NULL)
    negative <- parens || (length(signs) == 1 && signs != "+")
    # (A `-` in chartr() is a range, so the signs are replaced on their own.)
    sup <- chartr(
        "\u2070\u00b9\u00b2\u00b3\u2074\u2075\u2076\u2077\u2078\u2079", "0123456789", m[5]
    )
    sup <- sub("^\u207a", "+", sub("^\u207b", "-", sup))
    exp_text <- c(m[4], sup, m[6], "0")
    exp <- as.integer(gsub("\u2212", "-", exp_text[nzchar(exp_text)][1], fixed = TRUE))
    scales <- 0
    for (mark in names(.gw_per)) {
        if (grepl(mark, after, fixed = TRUE)) scales <- .gw_per[[mark]]
    }
    if (after %in% names(.gw_compact)) scales <- c(.gw_compact[[after]], 0)

    sign <- if (negative) "-" else ""
    readings <- list()
    for (r in .gw_readings(m[3])) {
        for (scale in scales) {
            power <- exp - r$decimals + scale
            # The display shows the value to its last digit that isn't zero.
            zeros <- if (grepl("[1-9]", r$digits)) {
                nchar(r$digits) - nchar(sub("0+$", "", r$digits))
            } else {
                0
            }
            readings[[length(readings) + 1]] <- list(
                digits = paste0(sign, r$digits),
                power = power,
                plain_int = r$decimals == 0 && exp == 0 && scale == 0,
                value = as.numeric(paste0(sign, r$digits, "e", power)),
                unit = 10^(power + zeros)
            )
        }
    }
    number <- suppressWarnings(as.numeric(raw))
    fits <- Filter(function(r) {
        isTRUE(abs(number - r$value) <= 0.5 * r$unit * (1 + 1e-9) + abs(number) * 1e-12)
    }, readings)
    chosen <- if (length(fits) > 0) fits else readings
    if (length(chosen) == 0) return(NULL)
    if (length(unique(vapply(chosen, function(r) r$value, numeric(1)))) > 1) return(NULL)
    chosen[[1]][c("digits", "power", "plain_int")]
}
