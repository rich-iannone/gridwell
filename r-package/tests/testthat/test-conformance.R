minimal_json <- function() load_fixture("minimal/minimal_1x1.json")

# The minimal table with its one body cell saying `text` (and, given `lang`,
# that language).
changed_json <- function(text = "Hello", lang = NULL) {
    json <- sub('"value": "Hello" }', paste0('"value": "', text, '" }'), minimal_json(), fixed = TRUE)
    if (!is.null(lang)) {
        json <- sub('"locale": "en-US",', paste0('"locale": "en-US", "lang": "', lang, '",'),
                    json, fixed = TRUE)
    }
    json
}

kit_dir <- file.path(testthat::test_path(), "..", "..", "..", "conformance")

# ---- gw_conformance_normalize() ----

test_that("normalize writes a table's grid as JSON", {
    grid <- gw_conformance_normalize(gw_parse_ir(minimal_json()))
    expect_s3_class(grid, "json")
    expect_match(grid, '"format": "gridwell-conformance-grid/1"', fixed = TRUE)
    expect_match(grid, '"tier": "L3"', fixed = TRUE)
    expect_identical(gw_conformance_normalize(minimal_json()), grid)
    l1 <- gw_conformance_normalize(minimal_json(), tier = "l1")
    expect_match(l1, '"tier": "L1"', fixed = TRUE)
    expect_match(l1, '"text": "Hello"', fixed = TRUE)
    expect_no_match(l1, '"runs"', fixed = TRUE)
})

test_that("normalize refuses an invalid table or tier", {
    expect_error(gw_conformance_normalize(load_fixture("invalid/col_count_mismatch.json")),
                 "COL_COUNT")
    expect_error(gw_conformance_normalize(minimal_json(), tier = "L4"),
                 'unknown tier "L4"; tiers are L0, L1, L2 and L3', fixed = TRUE)
    expect_error(gw_conformance_normalize(minimal_json(), tier = c("L1", "L2")),
                 "`tier` must be a single string")
    expect_error(gw_conformance_normalize(minimal_json(), tier = NA_character_),
                 "`tier` must be a single string")
    expect_error(gw_conformance_normalize(42))
})

# ---- gw_conformance_check() ----

test_that("a table passes its own grid, given as JSON or a file", {
    grid <- gw_conformance_normalize(minimal_json())
    path <- tempfile(fileext = ".json")
    on.exit(unlink(path))
    writeLines(grid, path, useBytes = TRUE)
    for (expected in list(grid, as.character(grid), paste0("\n  ", grid), path)) {
        report <- gw_conformance_check(minimal_json(), expected)
        expect_s3_class(report, "gw_conformance_report")
        expect_named(report, c("passed", "tier", "tiers", "errors", "differences", "text"))
        expect_true(report$passed)
        expect_identical(report$tier, "L3")
        expect_identical(report$tiers, c(L0 = "pass", L1 = "pass", L2 = "pass", L3 = "pass"))
        expect_identical(report$errors, character(0))
        expect_identical(
            report$differences,
            data.frame(tier = character(0), path = character(0), expected = character(0),
                       actual = character(0), stringsAsFactors = FALSE)
        )
        expect_match(report$text, "^passes L3\n")
    }
    expect_true(gw_conformance_check(gw_parse_ir(minimal_json()), grid)$passed)
})

test_that("each difference is a row, NA where a grid has nothing", {
    grid <- gw_conformance_normalize(minimal_json())
    report <- gw_conformance_check(changed_json("Bye", lang = "fr"), grid)
    expect_false(report$passed)
    expect_identical(report$tiers, c(L0 = "pass", L1 = "fail", L2 = "fail", L3 = "fail"))
    d <- report$differences
    expect_identical(d$path, c("groups[0].rows[0].cells[0].runs[0].text",
                               "groups[0].rows[0].cells[0].text", "lang"))
    expect_identical(d$tier, c("L2", "L1", "L2"))
    # As JSON; the expected grid has no lang at all.
    expect_identical(d$expected, c('"Hello"', '"Hello"', NA))
    expect_identical(d$actual, c('"Bye"', '"Bye"', '"fr"'))
    # And the other way round.
    back <- gw_conformance_check(minimal_json(), gw_conformance_normalize(changed_json(lang = "fr")))
    expect_identical(back$differences$expected, '"fr"')
    expect_identical(back$differences$actual, NA_character_)
    expect_match(back$text, 'L2 lang: expected "fr", got nothing', fixed = TRUE)
})

test_that("a report prints and formats as its text", {
    report <- gw_conformance_check(changed_json("Bye"), gw_conformance_normalize(minimal_json()))
    expect_identical(format(report), report$text)
    expect_match(report$text, "^fails L1 \\(checked to L3\\)\n")
    expect_match(report$text,
                 'L1 groups[0].rows[0].cells[0].text: expected "Hello", got "Bye"', fixed = TRUE)
    out <- capture.output(res <- withVisible(print(report)))
    expect_identical(paste0(paste(out, collapse = "\n"), "\n"), report$text)
    expect_false(res$visible)
    expect_identical(res$value, report)
})

test_that("a check goes to the tier asked for, up to the grid's", {
    grid <- gw_conformance_normalize(minimal_json())
    report <- gw_conformance_check(changed_json(lang = "fr"), grid, tier = "L1")
    expect_true(report$passed)
    expect_identical(report$tier, "L1")
    expect_identical(report$tiers,
                     c(L0 = "pass", L1 = "pass", L2 = "not checked", L3 = "not checked"))
    l1 <- gw_conformance_normalize(minimal_json(), tier = "L1")
    expect_identical(gw_conformance_check(changed_json(lang = "fr"), l1)$tier, "L1")
    expect_true(gw_conformance_check(changed_json(lang = "fr"), l1)$passed)
    expect_error(gw_conformance_check(minimal_json(), l1, tier = "L2"),
                 "goes to L1, so it can't be checked to L2", fixed = TRUE)
    expect_error(gw_conformance_check(minimal_json(), grid, tier = "x"), "unknown tier")
    for (bad in list(2, c("L1", "L2"), NA_character_)) {
        expect_error(gw_conformance_check(minimal_json(), grid, tier = bad),
                     "`tier` must be NULL or a single string")
    }
})

test_that("IR that doesn't parse or validate fails L0", {
    grid <- gw_conformance_normalize(minimal_json())
    for (ir in c("{not json", load_fixture("invalid/col_count_mismatch.json"))) {
        report <- gw_conformance_check(ir, grid)
        expect_false(report$passed)
        expect_identical(report$tiers[["L0"]], "fail")
        expect_true(length(report$errors) > 0)
        expect_identical(nrow(report$differences), 0L)
    }
    expect_match(gw_conformance_check("{not json", grid)$errors, "^the IR doesn't parse: ")
})

test_that("an expected grid that isn't one is an error", {
    ir <- minimal_json()
    expect_error(gw_conformance_check(ir, ir), "the expected grid has no format", fixed = TRUE)
    expect_error(gw_conformance_check(ir, "{oops"), "the expected grid isn't JSON", fixed = TRUE)
    expect_error(gw_conformance_check(ir, "no/such/grid.json"),
                 "`expected` isn't JSON or a file: no/such/grid.json", fixed = TRUE)
    for (bad in list(7, NA_character_, c("{}", "{}"), NULL)) {
        expect_error(gw_conformance_check(ir, bad), "`expected` must be a grid")
    }
    expect_error(gw_conformance_check(42, gw_conformance_normalize(ir)))
})

test_that("a gt table is checked as its IR", {
    skip_if_not_installed("gt")
    tbl <- gt::gt(head(mtcars, 2))
    grid <- gw_conformance_normalize(gt_to_ir(tbl))
    expect_identical(gw_conformance_normalize(tbl), grid)
    expect_true(gw_conformance_check(tbl, grid)$passed)
    expect_false(gw_conformance_check(gt::gt(head(mtcars, 3)), grid)$passed)
})

# ---- The kit ----

test_that("the gt reference IR conforms to expected/gt", {
    skip_if_not(dir.exists(kit_dir), "not in a source checkout")
    ref_dir <- file.path(kit_dir, "..", "harness", "gt-reference")
    files <- list.files(file.path(kit_dir, "expected", "gt"), pattern = "\\.json$")
    expect_gt(length(files), 0)
    for (file in files) {
        name <- sub("\\.json$", "", file)
        report <- gw_conformance_check(
            paste(readLines(file.path(ref_dir, paste0(name, ".ir.json")), encoding = "UTF-8"),
                  collapse = "\n"),
            file.path(kit_dir, "expected", "gt", file)
        )
        expect_true(report$passed, label = name, info = format(report))
    }
})

test_that("each case's gt table conforms to expected/gt", {
    # gt_to_ir() of the cases, live; only with the gt version the expected grids
    # were made with (another can format values differently).
    skip_if_not_installed("gt")
    skip_if_not_installed("jsonlite")
    skip_if_not(dir.exists(kit_dir), "not in a source checkout")
    ref_dir <- file.path(kit_dir, "..", "harness", "gt-reference")
    manifest <- jsonlite::fromJSON(file.path(ref_dir, "manifest.json"), simplifyVector = FALSE)
    skip_if_not(
        identical(as.character(utils::packageVersion("gt")), manifest$gt_version),
        paste("expected grids made with gt", manifest$gt_version)
    )
    env <- new.env()
    suppressPackageStartupMessages(library(gt))
    sys.source(file.path(kit_dir, "cases.R"), envir = env)
    expect_setequal(names(env$cases),
                    sub("\\.json$", "", list.files(file.path(kit_dir, "expected", "gt"))))
    emitter <- c(gt = "upstream", gridwell = "gridwell")[[manifest$ir_emitter]]
    for (name in names(env$cases)) {
        ir <- gt_to_ir(env$cases[[name]]$build(), emitter = emitter)
        report <- gw_conformance_check(ir, file.path(kit_dir, "expected", "gt", paste0(name, ".json")))
        expect_true(report$passed, label = name, info = format(report))
    }
})
