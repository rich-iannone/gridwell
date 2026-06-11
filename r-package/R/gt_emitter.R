#' Convert a gt table object to gridwell IR JSON
#'
#' @param gt_obj A `gt_tbl` object from the gt package.
#' @return A JSON string in gridwell IR format.
#' @export
gt_to_ir <- function(gt_obj) {
    ir <- gt_to_ir_list(gt_obj)
    jsonlite::toJSON(ir, auto_unbox = TRUE, null = "null", pretty = TRUE)
}

#' Convert a gt table object to a gridwell IR list
#'
#' @param gt_obj A `gt_tbl` object from the gt package.
#' @return A list representing the gridwell IR.
#' @export
gt_to_ir_list <- function(gt_obj) {

    # Extract gt internals
    data <- gt_obj[["_data"]]
    boxhead <- gt_obj[["_boxhead"]]
    stub_df <- gt_obj[["_stub_df"]]
    heading <- gt_obj[["_heading"]]
    row_groups <- gt_obj[["_row_groups"]]
    source_notes <- gt_obj[["_source_notes"]]
    footnotes <- gt_obj[["_footnotes"]]

    # Determine visible columns (not stub, not row_group, not hidden)
    is_default <- boxhead$type == "default"
    is_stub <- boxhead$type == "stub"
    has_stub <- any(is_stub)

    visible_cols <- boxhead$var[is_default]
    stub_var <- if (has_stub) boxhead$var[is_stub][1] else NULL

    # All display columns
    all_vars <- if (has_stub) c(stub_var, visible_cols) else visible_cols
    n_cols <- length(all_vars)
    stub_cols <- if (has_stub) 1L else 0L

    has_groups <- length(row_groups) > 0

    # Build config
    config <- list(
        table_cols = n_cols,
        header_rows = 1L,
        body_rows = nrow(data),
        stub_cols = stub_cols,
        row_striping = FALSE,
        row_striping_include_stub = FALSE,
        row_striping_include_body = FALSE,
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

    # Build column_spec
    column_spec <- lapply(all_vars, function(var) {
        idx <- which(boxhead$var == var)
        align <- boxhead$column_align[idx]
        width <- boxhead$column_width[[idx]]
        label <- boxhead$column_label[[idx]]
        if (is.null(label)) label <- var

        list(
            id = var,
            align = align,
            align_char = NULL,
            width = if (is.null(width)) "auto" else as.character(width),
            min_width = NULL,
            max_width = NULL,
            style_id = NULL,
            hidden = FALSE,
            label = as.character(label)
        )
    })

    # Build header
    header <- list(
        title = NULL,
        subtitle = NULL,
        extra_lines = list(),
        preheader_content = NULL
    )
    if (!is.null(heading$title) && nzchar(heading$title)) {
        header$title <- list(
            content = list(list(type = "text", value = heading$title))
        )
    }
    if (!is.null(heading$subtitle) && nzchar(heading$subtitle)) {
        header$subtitle <- list(
            content = list(list(type = "text", value = heading$subtitle))
        )
    }

    # Build thead
    thead_cells <- lapply(all_vars, function(var) {
        idx <- which(boxhead$var == var)
        label <- boxhead$column_label[[idx]]
        if (is.null(label)) label <- var

        list(
            content = list(list(type = "text", value = as.character(label))),
            colspan = 1L,
            rowspan = 1L,
            style_id = NULL,
            is_stub = FALSE,
            is_placeholder = FALSE,
            scope = "col",
            sort_key = NULL,
            data_type = NULL
        )
    })

    thead <- list(
        rows = list(list(
            role = "column_label",
            style_id = NULL,
            cells = thead_cells
        ))
    )

    # Build tbody
    tbody <- .build_tbody_r(data, all_vars, stub_df, has_stub, has_groups,
                            row_groups, stub_var)

    # Build footer
    footer <- list(
        footnotes = list(),
        source_notes = list()
    )
    if (length(source_notes) > 0) {
        footer$source_notes <- lapply(source_notes, function(sn) {
            text <- if (is.list(sn)) as.character(sn[[1]]) else as.character(sn)
            list(content = list(list(type = "text", value = text)))
        })
    }

    # Build styles (empty for now - gt styles are complex)
    styles <- list(
        defs = setNames(list(), character(0)),
        compositions = setNames(list(), character(0)),
        conditionals = list()
    )

    list(
        ir_version = "1.0",
        config = config,
        styles = styles,
        header = header,
        column_spec = column_spec,
        table = list(thead = thead, tbody = tbody),
        footer = footer,
        extensions = setNames(list(), character(0))
    )
}

.build_tbody_r <- function(data, all_vars, stub_df, has_stub, has_groups,
                           row_groups, stub_var) {
    if (has_groups && length(row_groups) > 0) {
        groups <- lapply(row_groups, function(grp_id) {
            row_indices <- which(stub_df$group_id == grp_id)
            rows <- lapply(row_indices, function(i) {
                .build_row_r(data, i, all_vars, has_stub, stub_var)
            })

            # Group label
            grp_labels <- stub_df$group_label[row_indices]
            grp_label_text <- as.character(grp_labels[[1]])

            label <- list(
                content = list(list(type = "text", value = grp_label_text)),
                style_id = NULL,
                colspan = NULL
            )

            list(
                group_id = grp_id,
                label = label,
                rows = rows,
                summary_rows = list()
            )
        })
        return(groups)
    } else {
        rows <- lapply(seq_len(nrow(data)), function(i) {
            .build_row_r(data, i, all_vars, has_stub, stub_var)
        })
        return(list(list(
            group_id = NULL,
            label = NULL,
            rows = rows,
            summary_rows = list()
        )))
    }
}

.build_row_r <- function(data, row_idx, all_vars, has_stub, stub_var) {
    cells <- lapply(seq_along(all_vars), function(col_idx) {
        var <- all_vars[col_idx]
        is_stub_cell <- has_stub && col_idx == 1
        value <- data[[var]][row_idx]
        text <- if (is.na(value)) "" else as.character(value)

        data_type <- .infer_dtype_r(value)

        cell <- list(
            content = list(list(type = "text", value = text)),
            colspan = 1L,
            rowspan = 1L,
            style_id = NULL,
            is_stub = is_stub_cell,
            is_placeholder = FALSE,
            data_type = data_type,
            sort_key = if (nzchar(text)) tolower(text) else NULL
        )

        if (!is_stub_cell && nzchar(text)) {
            cell$typed_value <- list(
                type = if (is.null(data_type)) "string" else data_type,
                value = as.character(value)
            )
        }

        cell
    })

    list(
        role = NULL,
        style_id = NULL,
        cells = cells
    )
}

.infer_dtype_r <- function(value) {
    if (is.na(value)) return(NULL)
    if (is.numeric(value)) {
        if (is.integer(value)) return("integer")
        return("number")
    }
    if (is.logical(value)) return("boolean")
    if (inherits(value, "Date")) return("date")
    if (inherits(value, "POSIXt")) return("datetime")
    "string"
}
