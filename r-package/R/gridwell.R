#' @keywords internal
"_PACKAGE"

#' Parse a gridwell table from a JSON string.
#'
#' @param json A JSON string containing the table IR.
#' @return A `gw_table`: the parsed table. It prints as text in the console and
#'   renders as the document's format in R Markdown and Quarto, and every
#'   `gw_*()` function takes it. It keeps its IR, so it survives `saveRDS()` and
#'   `readRDS()` (and knitr's cache).
#'
#'   IR of a newer minor version than this gridwell reads (`"1.3"`, say) is
#'   read with a warning, leaving out what the newer version added; IR of
#'   another major version is refused.
#' @examples
#' \dontrun{
#' tbl <- gw_parse_ir(json)
#' tbl
#' gw_render_html(tbl)
#' }
#' @export
gw_parse_ir <- function(json) {
    if (inherits(json, "json")) {
        json <- as.character(json)
    }
    if (!is.character(json) || length(json) != 1 || is.na(json)) {
        stop("`json` must be a single string of table IR.", call. = FALSE)
    }
    json <- enc2utf8(json)
    ptr <- rs_parse_ir(json)
    for (w in rs_version_warning(ptr)) {
        warning(w, call. = FALSE)
    }
    new_gw_table(ptr, json)
}

new_gw_table <- function(ptr, json) {
    x <- new.env(parent = emptyenv())
    x$ptr <- ptr
    x$json <- json
    class(x) <- "gw_table"
    x
}

#' Make a gridwell table.
#'
#' Every `gw_*()` function calls this on its table argument, so each takes any
#' of these.
#'
#' @param x A `gw_table` (from [gw_parse_ir()]), returned as it is; a gt table
#'   (a `gt_tbl`, converted with [gt_to_ir()]); or table IR as a JSON string.
#' @param ... Unused.
#' @return A `gw_table`.
#' @examples
#' \dontrun{
#' as_gw_table(gt::gt(head(mtcars)))
#' }
#' @export
as_gw_table <- function(x, ...) {
    UseMethod("as_gw_table")
}

#' @rdname as_gw_table
#' @export
as_gw_table.gw_table <- function(x, ...) x

#' @rdname as_gw_table
#' @export
as_gw_table.gt_tbl <- function(x, ...) gw_parse_ir(gt_to_ir(x))

#' @rdname as_gw_table
#' @export
as_gw_table.character <- function(x, ...) gw_parse_ir(x)

#' @rdname as_gw_table
#' @export
as_gw_table.json <- function(x, ...) gw_parse_ir(x)

#' @rdname as_gw_table
#' @export
as_gw_table.default <- function(x, ...) {
    stop(
        "Expected a gridwell table: a `gw_table` from gw_parse_ir(), a gt table, ",
        "or table IR as a JSON string; got <", paste(class(x), collapse = "/"), ">.",
        call. = FALSE
    )
}

# The table's pointer, for the Rust functions: parsed again from its IR when
# `readRDS()` gave back a pointer to nothing.
gw_ptr <- function(x) {
    x <- as_gw_table(x)
    if (!rs_is_table(x$ptr)) {
        x$ptr <- rs_parse_ir(x$json)
    }
    x$ptr
}

#' Validate a parsed table.
#'
#' @param x A table: a `gw_table` from [gw_parse_ir()], a gt table, or table IR
#'   as a JSON string (see [as_gw_table()]).
#' @return A character vector of validation errors (empty if valid).
#' @export
gw_validate <- function(x) {
    rs_validate(gw_ptr(x))
}

#' Find what output formats can't express of a parsed table.
#'
#' Some formats lack a feature the IR has (LaTeX can't align a single cell to
#' the bottom of its row, Typst has no double borders); the writer then draws
#' something close. Each warning names the format, the feature, where the
#' table uses it, and what's drawn instead. A warning doesn't stop rendering.
#'
#' A table of a newer minor IR version than this gridwell reads gets a warning
#' first, for any format: what that version added is left out.
#'
#' @param x A table: a `gw_table` from [gw_parse_ir()], a gt table, or table IR
#'   as a JSON string (see [as_gw_table()]).
#' @param format A format name (see [gw_formats()]), or `NULL` for every format.
#' @return A character vector of warnings (empty if the table is IR of this
#'   gridwell's version and every feature it uses is expressed). Invalid IR,
#'   and an unknown format, are errors.
#' @export
gw_warnings <- function(x, format = NULL) {
    rs_warnings(gw_ptr(x), format)
}

#' Serialize a parsed table back to JSON.
#'
#' @param x A table: a `gw_table` from [gw_parse_ir()], a gt table, or table IR
#'   as a JSON string (see [as_gw_table()]).
#' @return A JSON string.
#' @export
gw_to_json <- function(x) {
    rs_to_json(gw_ptr(x))
}

#' List the supported output formats.
#'
#' @return A data frame with one row per format: `name`, `description`, `kind`
#'   (`"text"` or `"binary"`), `extension`, `media_type`, `experimental` (`TRUE`
#'   for a format whose styling is best-effort and may change between releases),
#'   and `options` (the format's options and their defaults, as a JSON string).
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
#' @param x A table: a `gw_table` from [gw_parse_ir()], a gt table, or table IR
#'   as a JSON string (see [as_gw_table()]).
#' @param format A text format name (see [gw_formats()]): "html", "latex",
#'   "typst", "rtf", "svg", "ansi", "pandoc" or "quarto".
#' @param options Writer options: `NULL` for the defaults, a named list, or a JSON
#'   string. Unknown options are an error. See [gw_formats()] for each format's
#'   options.
#' @return The rendered string.
#' @export
gw_render <- function(x, format, options = NULL) {
    rs_render_text(gw_ptr(x), format, gw_options_json(options))
}

#' Render a table to a binary format.
#'
#' @inheritParams gw_render
#' @param format A binary format name: "docx", "xlsx" or "pptx".
#' @return A raw vector containing the file bytes.
#' @export
gw_render_binary <- function(x, format, options = NULL) {
    rs_render_binary(gw_ptr(x), format, gw_options_json(options))
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
#' @param x A table: a `gw_table` from [gw_parse_ir()], a gt table, or table IR
#'   as a JSON string (see [as_gw_table()]).
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
    rs_save(gw_ptr(x), enc2utf8(path.expand(path)), format, gw_options_json(list(...)))
    invisible(path)
}

#' Render a table to a specific format.
#'
#' Shorthands for [gw_render()] and [gw_render_binary()]; writer options are
#' passed as named arguments, e.g. `gw_render_html(tbl, inline_styles = TRUE)`.
#' Every format takes `raw`: for IR you don't trust, `raw = FALSE` writes no raw
#' markup from it (raw HTML is drawn as text and styled runs instead).
#'
#' @param x A table: a `gw_table` from [gw_parse_ir()], a gt table, or table IR
#'   as a JSON string (see [as_gw_table()]).
#' @param ... Writer options as named arguments (see [gw_formats()]).
#' @return A string for text formats, a raw vector for binary formats.
#' @name gw_render_format
NULL

#' @rdname gw_render_format
#' @export
gw_render_html <- function(x, ...) gw_render(x, "html", list(...))

#' @rdname gw_render_format
#' @export
gw_render_latex <- function(x, ...) gw_render(x, "latex", list(...))

#' @rdname gw_render_format
#' @export
gw_render_typst <- function(x, ...) gw_render(x, "typst", list(...))

#' @rdname gw_render_format
#' @export
gw_render_rtf <- function(x, ...) gw_render(x, "rtf", list(...))

#' @rdname gw_render_format
#' @export
gw_render_svg <- function(x, ...) gw_render(x, "svg", list(...))

#' @rdname gw_render_format
#' @export
gw_render_ansi <- function(x, ...) gw_render(x, "ansi", list(...))

#' @rdname gw_render_format
#' @export
gw_render_pandoc <- function(x, ...) gw_render(x, "pandoc", list(...))

#' @rdname gw_render_format
#' @export
gw_render_quarto <- function(x, ...) gw_render(x, "quarto", list(...))

#' @rdname gw_render_format
#' @export
gw_render_docx <- function(x, ...) gw_render_binary(x, "docx", list(...))

#' @rdname gw_render_format
#' @export
gw_render_xlsx <- function(x, ...) gw_render_binary(x, "xlsx", list(...))

#' @rdname gw_render_format
#' @export
gw_render_pptx <- function(x, ...) gw_render_binary(x, "pptx", list(...))
