# Delegation: gt_to_ir() uses gt's own emitter (gt::as_ir()) when the installed gt
# has one, and gridwell's copy otherwise. No released gt has as_ir() yet, so these
# tests give gridwell one.

skip_if_not_installed("gt")
skip_if_not_installed("jsonlite")

# IR unlike anything gridwell's emitter writes for the test table, laid out unlike
# it too (indented by 3, so a converter that re-serializes it is caught).
upstream_ir <- function(version = "1.0", title = "From gt") {
    jsonlite::toJSON(list(
        ir_version = version,
        config = list(table_cols = 1, body_rows = 1, header_rows = 1),
        styles = list(defs = setNames(list(), character(0)),
                      compositions = setNames(list(), character(0)), conditionals = list()),
        header = list(title = list(content = list(list(type = "text", value = title)))),
        column_spec = list(list(id = "x")),
        table = list(
            thead = list(rows = list(list(cells = list(list(
                content = list(list(type = "text", value = "x"))
            ))))),
            tbody = list(list(rows = list(list(cells = list(list(
                content = list(list(type = "text", value = "1"))
            ))))))
        )
    ), auto_unbox = TRUE, pretty = 3)
}

the_table <- function() gt::gt(gt::exibble[1:3, ]) |> gt::tab_header(title = "From gridwell")

# Give gridwell a gt::as_ir() that returns `result` (or signals it, for a
# condition), recording the tables it's called with.
give_as_ir <- function(result, env = parent.frame()) {
    calls <- new.env()
    calls$tables <- list()
    as_ir <- function(data) {
        calls$tables[[length(calls$tables) + 1]] <- data
        if (inherits(result, "error")) stop(result)
        if (inherits(result, "condition")) signalCondition(result)
        result
    }
    local_mocked_bindings(.gw_upstream_emitter = function(...) as_ir, .package = "gridwell",
                          .env = env)
    calls
}

ours <- function(tbl) as.character(gt_to_ir(tbl, emitter = "gridwell"))

# ---- Without an upstream emitter ----

test_that("gt has no as_ir() yet, and gridwell finds one that's there", {
    # When gt gains one, these tests still hold; the reference set then switches.
    expect_null(gridwell:::.gw_upstream_emitter())
    expect_identical(gridwell:::.gw_upstream_emitter("jsonlite", "toJSON"), jsonlite::toJSON)
    # Not exported, not there, or not a function.
    expect_null(gridwell:::.gw_upstream_emitter("jsonlite", "asJSON"))
    expect_null(gridwell:::.gw_upstream_emitter("gridwell.no.such.package", "as_ir"))
    expect_null(gridwell:::.gw_upstream_emitter("base", "pi"))
})

test_that("auto uses gridwell's own without a warning", {
    tbl <- the_table()
    expect_no_warning(json <- gt_to_ir(tbl))
    expect_identical(as.character(json), ours(tbl))
    expect_s3_class(json, "json")
    expect_identical(gt_to_ir_list(tbl), gt_to_ir_list(tbl, emitter = "gridwell"))
})

test_that("upstream without as_ir() is an error", {
    for (convert in list(gt_to_ir, gt_to_ir_list)) {
        expect_error(convert(the_table(), emitter = "upstream"),
                     "^gt [0-9.]+ has no `as_ir\\(\\)`; use `emitter = \"gridwell\"`")
    }
})

# ---- With one ----

test_that("auto uses as_ir() when there", {
    tbl <- the_table()
    calls <- give_as_ir(upstream_ir())
    expect_no_warning(json <- gt_to_ir(tbl))
    # Its JSON as it is, and the same parsed.
    expect_identical(as.character(json), as.character(upstream_ir()))
    expect_s3_class(json, "json")
    expect_identical(gt_to_ir_list(tbl), jsonlite::parse_json(upstream_ir()))
    expect_identical(as.character(gt_to_ir(tbl, emitter = "upstream")), as.character(upstream_ir()))
    expect_length(calls$tables, 3)
    for (seen in calls$tables) expect_identical(seen, tbl)
})

test_that("tables made from a gt table use as_ir()", {
    give_as_ir(upstream_ir())
    expect_no_warning(tbl <- as_gw_table(the_table()))
    expect_identical(gw_validate(tbl), character(0))
    expect_match(gw_render_html(tbl), "From gt")
    expect_match(gw_render_html(the_table()), "From gt")
})

test_that("gridwell ignores as_ir()", {
    tbl <- the_table()
    expected <- ours(tbl)
    expected_list <- gt_to_ir_list(tbl, emitter = "gridwell")
    calls <- give_as_ir(upstream_ir())
    expect_identical(as.character(gt_to_ir(tbl, emitter = "gridwell")), expected)
    expect_identical(gt_to_ir_list(tbl, emitter = "gridwell"), expected_list)
    expect_length(calls$tables, 0)
})

test_that("any IR 1 minor version is used", {
    for (version in c("1.0", "1.1", "1.10", "01.0")) {
        give_as_ir(upstream_ir(version))
        expect_no_warning(ir <- gt_to_ir_list(the_table()))
        expect_identical(ir$ir_version, version)
    }
})

ir_file <- tempfile(fileext = ".json")
writeLines(upstream_ir(), ir_file)

# Each bad result, and the reason given for not using it.
bad <- list(
    "IR 2" = list(upstream_ir("2.0"), "gives IR 2\\.0, and this gridwell reads IR 1\\.x"),
    "IR 0" = list(upstream_ir("0.9"), "gives IR 0\\.9, and this gridwell reads IR 1\\.x"),
    "IR 00" = list(upstream_ir("00.1"), "gives IR 00\\.1"),
    "IR 10" = list(upstream_ir("10.0"), "gives IR 10\\.0"),
    "major only" = list(upstream_ir("1"), "an ir_version of \"1\", not MAJOR\\.MINOR"),
    "three parts" = list(upstream_ir("1.0.0"), "an ir_version of \"1\\.0\\.0\""),
    "padded" = list(upstream_ir(" 1.0"), "an ir_version of \" 1\\.0\""),
    "trailing newline" = list(upstream_ir("1.0\n"), "an ir_version of \"1\\.0\\\\n\""),
    "other digits" = list(upstream_ir("\u0661.\u0660"), "an ir_version of .*, not MAJOR\\.MINOR"),
    "number" = list(upstream_ir(1.5), "an ir_version of a numeric \\(1\\.5\\), not"),
    "no version" = list('{"config": {}}', "an ir_version of null"),
    "JSON array" = list('[{"ir_version": "1.0"}]', "an ir_version of null"),
    "JSON string" = list('"1.0"', "an ir_version of null"),
    "not JSON" = list("<table></table>", "JSON that doesn't parse \\(lexical error"),
    # Strings that name IR elsewhere are not read.
    "a URL" = list("https://example.invalid/ir.json", "JSON that doesn't parse \\(lexical error"),
    "a file" = list(ir_file, "JSON that doesn't parse \\(lexical error"),
    "two strings" = list(c(upstream_ir(), upstream_ir()),
                         "gave a character of length 2, not a JSON string"),
    "NA" = list(NA_character_, "gave a character \\(NA\\), not a JSON string"),
    "list" = list(jsonlite::parse_json(upstream_ir()), "gave a list of length 6"),
    "NULL" = list(NULL, "gave null, not a JSON string")
)

test_that("auto warns and uses its own for a bad result", {
    tbl <- the_table()
    expected <- ours(tbl)
    expected_list <- gt_to_ir_list(tbl, emitter = "gridwell")
    for (case in names(bad)) {
        give_as_ir(bad[[case]][[1]])
        reason <- bad[[case]][[2]]
        expect_warning(json <- gt_to_ir(tbl),
                       paste0(reason, ".*; converted with gridwell's own emitter instead$"),
                       label = case)
        expect_identical(as.character(json), expected, label = case)
        expect_warning(ir <- gt_to_ir_list(tbl), reason, label = case)
        expect_identical(ir, expected_list, label = case)
        expect_warning(html <- gw_render_html(tbl), reason, label = case)
        expect_match(html, "From gridwell", label = case)
    }
})

test_that("upstream errors for a bad result", {
    for (case in names(bad)) {
        give_as_ir(bad[[case]][[1]])
        for (convert in list(gt_to_ir, gt_to_ir_list)) {
            expect_error(convert(the_table(), emitter = "upstream"), bad[[case]][[2]],
                         label = case)
        }
    }
})

test_that("an error in as_ir()", {
    tbl <- the_table()
    give_as_ir(simpleError("no such column"))
    w <- expect_warning(json <- gt_to_ir(tbl),
                        "^no such column; converted with gridwell's own emitter instead$")
    # Not from a gridwell internal.
    expect_null(conditionCall(w))
    expect_identical(as.character(json), ours(tbl))
    # Its own error, as signaled.
    expect_error(gt_to_ir(tbl, emitter = "upstream"), "^no such column$")
})

test_that("only errors are caught", {
    give_as_ir(structure(class = c("interrupt", "condition"), list(message = "", call = NULL)))
    # It reaches the caller's handlers. (A real one would stop there; this one goes on
    # to give the condition, which isn't IR.)
    expect_condition(suppressWarnings(gt_to_ir(the_table())), class = "interrupt")
})

# ---- Arguments ----

test_that("an unknown emitter is an error", {
    calls <- give_as_ir(upstream_ir())
    for (convert in list(gt_to_ir, gt_to_ir_list)) {
        for (emitter in c("great_tables", "Auto", "")) {
            expect_error(convert(the_table(), emitter = emitter), "should be one of")
        }
    }
    expect_length(calls$tables, 0)
})

test_that("anything but a gt table is an error", {
    give_as_ir(upstream_ir())
    for (convert in list(gt_to_ir, gt_to_ir_list)) {
        for (emitter in c("auto", "upstream", "gridwell")) {
            for (other in list(gt::exibble, upstream_ir(), NULL)) {
                expect_error(convert(other, emitter = emitter),
                             "`gt_obj` must be a gt table \\(a `gt_tbl` object\\)")
            }
        }
    }
})
