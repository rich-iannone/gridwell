#' Conformance: whether a table's IR says what an expected table says.
#'
#' Two correct emitters can write the same table very differently (other style
#' ids, a composition instead of a definition, `red` or `#FF0000`, keys in
#' another order), so tables are compared as normalized grids: what a reader
#' sees, with every style its resolved properties and footnote marks by
#' footnote number. `gw_conformance_normalize()` writes a table's grid;
#' `gw_conformance_check()` compares it with an expected one, in tiers that
#' each include the ones before:
#'
#' - L0 valid: the IR parses and validates.
#' - L1 structure: columns and stub, header lines, head rows and spans, groups
#'   and their labels, row roles, data and summary rows, cell text.
#' - L2 content: runs, footnote marks, typed values, `scope`,
#'   column and group ids, `lang` and `dir`.
#' - L3 styling: resolved styles; alignment; column, table and image sizes.
#'
#' An emitter's tests can check it against the expected grids in gridwell's
#' `conformance/` directory. The CLI has the same: `gridwell conformance
#' normalize` and `gridwell conformance check`.
#'
#' @param x A table: a `gw_table` from [gw_parse_ir()], a gt table, or table IR
#'   as a JSON string (see [as_gw_table()]). For `gw_conformance_check()`, IR
#'   that doesn't parse or validate fails L0 rather than being an error.
#' @param tier The tier: `"L0"` to `"L3"`. For `gw_conformance_check()`, `NULL`
#'   checks to the tier the expected grid goes to.
#' @param expected The expected grid: a JSON string (a string that starts with a
#'   brace), or the path of a grid file.
#' @return `gw_conformance_normalize()`: the grid, as a JSON string (of class
#'   `json`). An invalid table is an error.
#'
#'   `gw_conformance_check()`: a `gw_conformance_report`, a list of `passed`
#'   (whether the table passes the tier checked), `tier` (the tier checked to),
#'   `tiers` (each tier's status: `"pass"`, `"fail"` or `"not checked"`; a tier
#'   passes when it and every tier below it do), `errors` (why the IR fails
#'   L0), `differences` (a data frame of `tier`, `path`, and `expected` and
#'   `actual` as JSON, `NA` when that grid has nothing there) and `text` (the
#'   report, as printed). An expected grid that isn't one, or a tier it
#'   doesn't go to, is an error.
#' @examples
#' \dontrun{
#' tbl <- gt::gt(head(mtcars))
#' grid <- gw_conformance_normalize(tbl, tier = "L1")
#' report <- gw_conformance_check(tbl, grid)
#' report
#' # In a test:
#' testthat::expect_true(report$passed, info = format(report))
#' }
#' @name gw_conformance
NULL

#' @rdname gw_conformance
#' @export
gw_conformance_normalize <- function(x, tier = "L3") {
    if (!is.character(tier) || length(tier) != 1 || is.na(tier)) {
        stop("`tier` must be a single string.", call. = FALSE)
    }
    structure(rs_conformance_normalize(gw_ptr(x), tier), class = "json")
}

#' @rdname gw_conformance
#' @export
gw_conformance_check <- function(x, expected, tier = NULL) {
    if (!is.null(tier) && (!is.character(tier) || length(tier) != 1 || is.na(tier))) {
        stop("`tier` must be NULL or a single string.", call. = FALSE)
    }
    r <- rs_conformance_check(gw_ir_json(x), gw_grid_json(expected), tier)
    structure(
        list(
            passed = r$passed,
            tier = r$tier,
            tiers = stats::setNames(r$tiers, c("L0", "L1", "L2", "L3")),
            errors = r$errors,
            differences = data.frame(
                tier = r$diff_tier, path = r$diff_path,
                expected = r$diff_expected, actual = r$diff_actual,
                stringsAsFactors = FALSE
            ),
            text = r$text
        ),
        class = "gw_conformance_report"
    )
}

#' @export
format.gw_conformance_report <- function(x, ...) {
    x$text
}

#' @export
print.gw_conformance_report <- function(x, ...) {
    cat(x$text)
    invisible(x)
}

# A table's IR as JSON text, as it is: IR that doesn't parse is the check's to
# report (it fails L0).
gw_ir_json <- function(x) {
    if (inherits(x, "gw_table")) {
        return(x$json)
    }
    if (inherits(x, "gt_tbl")) {
        return(as.character(gt_to_ir(x)))
    }
    if (is.character(x) && length(x) == 1 && !is.na(x)) {
        return(enc2utf8(x))
    }
    as_gw_table.default(x)
}

gw_grid_json <- function(expected) {
    if (!is.character(expected) || length(expected) != 1 || is.na(expected)) {
        stop("`expected` must be a grid as a JSON string, or the path of a grid file.",
             call. = FALSE)
    }
    if (grepl("^\\s*\\{", expected)) {
        return(enc2utf8(expected))
    }
    if (!file.exists(expected)) {
        stop("`expected` isn't JSON or a file: ", expected, call. = FALSE)
    }
    paste(readLines(expected, encoding = "UTF-8", warn = FALSE), collapse = "\n")
}
