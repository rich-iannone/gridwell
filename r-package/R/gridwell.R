#' @keywords internal
"_PACKAGE"

#' Parse a gridwell table from a JSON string.
#'
#' @param json A JSON string containing the table IR.
#' @return An external pointer to the parsed table.
#' @export
gw_parse_ir <- function(json) {
    rs_parse_ir(json)
}

#' Validate a parsed table.
#'
#' @param table_ptr An external pointer to a parsed table (from `gw_parse_ir()`).
#' @return A character vector of validation errors (empty if valid).
#' @export
gw_validate <- function(table_ptr) {
    rs_validate(table_ptr)
}

#' Serialize a parsed table back to JSON.
#'
#' @param table_ptr An external pointer to a parsed table.
#' @return A JSON string.
#' @export
gw_to_json <- function(table_ptr) {
    rs_to_json(table_ptr)
}

#' List the supported output formats.
#'
#' @return A data frame with one row per format: `name`, `description`, `kind`
#'   (`"text"` or `"binary"`), `extension`, `media_type`, and `options` (the
#'   format's options and their defaults, as a JSON string).
#' @export
gw_formats <- function() {
    as.data.frame(rs_formats(), stringsAsFactors = FALSE)
}

# Writer options as a JSON string: `NULL`, a JSON string, or a named list
# (converted with jsonlite).
gw_options_json <- function(options) {
    if (is.null(options) || (is.list(options) && length(options) == 0)) {
        return(NULL)
    }
    if (is.character(options) && length(options) == 1) {
        return(options)
    }
    if (is.list(options)) {
        if (is.null(names(options)) || any(names(options) == "")) {
            stop("`options` must be a named list (or a JSON string).", call. = FALSE)
        }
        if (!requireNamespace("jsonlite", quietly = TRUE)) {
            stop(
                "Passing `options` as a list needs the jsonlite package; ",
                "install it or pass a JSON string.",
                call. = FALSE
            )
        }
        return(as.character(jsonlite::toJSON(options, auto_unbox = TRUE, null = "null")))
    }
    stop("`options` must be NULL, a named list, or a JSON string.", call. = FALSE)
}

#' Render a table to a text format.
#'
#' The table is validated first; invalid IR is an error listing every problem.
#'
#' @param table_ptr An external pointer to a parsed table.
#' @param format A text format name (see [gw_formats()]): "html", "latex",
#'   "typst", "rtf", "svg", "ansi", "pandoc" or "quarto".
#' @param options Writer options: `NULL` for the defaults, a named list, or a JSON
#'   string. Unknown options are an error. See [gw_formats()] for each format's
#'   options.
#' @return The rendered string.
#' @export
gw_render <- function(table_ptr, format, options = NULL) {
    rs_render_text(table_ptr, format, gw_options_json(options))
}

#' Render a table to a binary format.
#'
#' @inheritParams gw_render
#' @param format A binary format name: "docx", "xlsx" or "pptx".
#' @return A raw vector containing the file bytes.
#' @export
gw_render_binary <- function(table_ptr, format, options = NULL) {
    rs_render_binary(table_ptr, format, gw_options_json(options))
}

#' Render a table and write it to a file.
#'
#' One call for every format: the format comes from the file extension, and the
#' rendered bytes are written unchanged (text as UTF-8, with no re-encoding and no
#' added newline), so there is no choosing between `writeLines()` and
#' `writeBin()`.
#'
#' The extensions are `.html`, `.tex` (latex), `.typ` (typst), `.rtf`, `.svg`,
#' `.txt` (ansi), `.docx`, `.xlsx` and `.pptx`, in any case. `.json` is both
#' "pandoc" and "quarto", so it needs `format`. A `.txt` file is plain text (no
#' ANSI escape codes) unless you pass `color = TRUE`. Nothing is written if the
#' table, format or options are invalid.
#'
#' @param x A gt table (a `gt_tbl`, converted with [gt_to_ir()]) or a parsed
#'   table from [gw_parse_ir()].
#' @param path The file to write.
#' @param format A format name (see [gw_formats()]) to use instead of the one the
#'   extension implies, or `NULL` to infer it.
#' @param ... Writer options as named arguments, as in [gw_render_html()] and
#'   the other `gw_render_*()` functions.
#' @return `path`, invisibly, so `gw_save()` works in pipes and with
#'   `purrr::walk()`.
#' @examples
#' \dontrun{
#' gt::gt(head(mtcars)) |> gw_save("mtcars.rtf")
#' tbl <- gw_parse_ir(json)
#' gw_save(tbl, "table.docx")
#' gw_save(tbl, "table.json", format = "quarto", table_id = "cars")
#' }
#' @export
gw_save <- function(x, path, format = NULL, ...) {
    if (!is.character(path) || length(path) != 1 || is.na(path) || !nzchar(path)) {
        stop("`path` must be a single file path.", call. = FALSE)
    }
    if (!is.null(format) && (!is.character(format) || length(format) != 1 || is.na(format))) {
        stop("`format` must be NULL or a single format name.", call. = FALSE)
    }
    if (inherits(x, "gt_tbl")) {
        x <- gw_parse_ir(gt_to_ir(x))
    }
    rs_save(x, enc2utf8(path.expand(path)), format, gw_options_json(list(...)))
    invisible(path)
}

#' Render a table to a specific format.
#'
#' Shorthands for [gw_render()] and [gw_render_binary()]; writer options are
#' passed as named arguments, e.g. `gw_render_html(tbl, inline_styles = TRUE)`.
#'
#' @param table_ptr An external pointer to a parsed table.
#' @param ... Writer options as named arguments (see [gw_formats()]).
#' @return A string for text formats, a raw vector for binary formats.
#' @name gw_render_format
NULL

#' @rdname gw_render_format
#' @export
gw_render_html <- function(table_ptr, ...) gw_render(table_ptr, "html", list(...))

#' @rdname gw_render_format
#' @export
gw_render_latex <- function(table_ptr, ...) gw_render(table_ptr, "latex", list(...))

#' @rdname gw_render_format
#' @export
gw_render_typst <- function(table_ptr, ...) gw_render(table_ptr, "typst", list(...))

#' @rdname gw_render_format
#' @export
gw_render_rtf <- function(table_ptr, ...) gw_render(table_ptr, "rtf", list(...))

#' @rdname gw_render_format
#' @export
gw_render_svg <- function(table_ptr, ...) gw_render(table_ptr, "svg", list(...))

#' @rdname gw_render_format
#' @export
gw_render_ansi <- function(table_ptr, ...) gw_render(table_ptr, "ansi", list(...))

#' @rdname gw_render_format
#' @export
gw_render_pandoc <- function(table_ptr, ...) gw_render(table_ptr, "pandoc", list(...))

#' @rdname gw_render_format
#' @export
gw_render_quarto <- function(table_ptr, ...) gw_render(table_ptr, "quarto", list(...))

#' @rdname gw_render_format
#' @export
gw_render_docx <- function(table_ptr, ...) gw_render_binary(table_ptr, "docx", list(...))

#' @rdname gw_render_format
#' @export
gw_render_xlsx <- function(table_ptr, ...) gw_render_binary(table_ptr, "xlsx", list(...))

#' @rdname gw_render_format
#' @export
gw_render_pptx <- function(table_ptr, ...) gw_render_binary(table_ptr, "pptx", list(...))
