fixtures_dir <- normalizePath(
    file.path(testthat::test_path(), "..", "..", "..", "fixtures"),
    mustWork = TRUE
)

load_fixture <- function(rel_path) {
    readLines(file.path(fixtures_dir, rel_path), warn = FALSE) |>
        paste(collapse = "\n")
}

# Set knitr's output format for one block, as rmarkdown and Quarto do.
with_knit_format <- function(pandoc_to, code, out_format = "markdown") {
    old <- knitr::opts_knit$get(c("rmarkdown.pandoc.to", "out.format"))
    knitr::opts_knit$set(rmarkdown.pandoc.to = pandoc_to, out.format = out_format)
    on.exit(knitr::opts_knit$set(old))
    force(code)
}

# JSON as a list with every object's names in order.
sort_names <- function(json) {
    sort_list <- function(x) {
        if (!is.list(x)) {
            return(x)
        }
        if (!is.null(names(x))) {
            x <- x[order(names(x))]
        }
        lapply(x, sort_list)
    }
    sort_list(jsonlite::fromJSON(json, simplifyVector = FALSE))
}

with_option <- function(name, value, code) {
    old <- options(stats::setNames(list(value), name))
    on.exit(options(old))
    force(code)
}

# ─── The gw_table object ───

test_that("every function takes a gw_table, IR as JSON, or a gt table", {
    json <- load_fixture("comprehensive/reference_table.json")
    tbl <- gw_parse_ir(json)
    expect_s3_class(tbl, "gw_table")
    expect_identical(as_gw_table(tbl), tbl)
    for (x in list(json, structure(json, class = "json"), tbl)) {
        expect_identical(gw_render_html(x), gw_render_html(tbl))
        expect_identical(gw_render(x, "latex"), gw_render_latex(tbl))
        expect_identical(gw_render_docx(x), gw_render_docx(tbl))
        expect_identical(gw_validate(x), character(0))
        expect_identical(gw_warnings(x, "typst"), gw_warnings(tbl, "typst"))
        # Style order in the JSON varies from parse to parse.
        expect_identical(sort_names(gw_to_json(x)), sort_names(gw_to_json(tbl)))
    }
    path <- tempfile(fileext = ".rtf")
    expect_identical(gw_save(json, path), path)
    expect_identical(readLines(path, warn = FALSE), strsplit(gw_render_rtf(tbl), "\n")[[1]])
})

test_that("a gt table converts on the way in", {
    skip_if_not_installed("gt")
    skip_if_not_installed("jsonlite")
    gt_tbl <- gt::gt(head(mtcars, 3)) |> gt::tab_header(title = "Cars")
    tbl <- as_gw_table(gt_tbl)
    expect_s3_class(tbl, "gw_table")
    expected <- gw_render_html(gw_parse_ir(gt_to_ir(gt_tbl)))
    expect_identical(gw_render_html(gt_tbl), expected)
    expect_identical(gw_render_html(tbl), expected)
    expect_match(format(tbl)[1], 'titled "Cars"', fixed = TRUE)
})

test_that("anything else is an error that says what a table is", {
    for (bad in list(42, NULL, list(), TRUE, mtcars)) {
        expect_error(as_gw_table(bad), "a `gw_table` from gw_parse_ir(), a gt table", fixed = TRUE)
        expect_error(gw_render_html(bad), "gw_parse_ir()", fixed = TRUE)
    }
    expect_error(as_gw_table(42), "got <numeric>", fixed = TRUE)
    expect_error(gw_parse_ir(c("{}", "{}")), "single string")
    expect_error(gw_parse_ir(NA_character_), "single string")
    expect_error(gw_render_html("{ nope"), "Failed to parse IR")
})

test_that("a table survives saveRDS() and readRDS()", {
    tbl <- gw_parse_ir(load_fixture("minimal/colspan_basic.json"))
    path <- tempfile(fileext = ".rds")
    saveRDS(tbl, path)
    back <- readRDS(path)
    expect_s3_class(back, "gw_table")
    # The pointer comes back empty, and is parsed again from the IR.
    expect_false(rs_is_table(back$ptr))
    expect_identical(gw_render_html(back), gw_render_html(tbl))
    expect_true(rs_is_table(back$ptr))
    expect_identical(format(back), format(tbl))
    expect_identical(gw_render_svg(unserialize(serialize(tbl, NULL))), gw_render_svg(tbl))
})

# ─── Printing ───

test_that("a table formats as its description and its drawing", {
    tbl <- gw_parse_ir(load_fixture("minimal/colspan_basic.json"))
    lines <- format(tbl, width = 0)
    expect_match(lines[1], "^<gw_table: [0-9]+ columns, [0-9]+ rows")
    expect_identical(lines[2], "")
    drawing <- gw_render_ansi(tbl, color = FALSE, true_color = FALSE)
    expect_identical(paste0(paste(lines[-(1:2)], collapse = "\n"), "\n"), drawing)
    expect_false(any(grepl("\x1b", lines, fixed = TRUE)))
    expect_true(any(grepl("\x1b", format(tbl, color = TRUE), fixed = TRUE)))
})

test_that("the drawing fits the width", {
    tbl <- gw_parse_ir(load_fixture("comprehensive/reference_table.json"))
    wide <- format(tbl, width = 0)[-(1:2)]
    expect_gt(max(nchar(wide)), 50)
    narrow <- format(tbl, width = 50)[-(1:2)]
    expect_lte(max(nchar(narrow)), 50)
    expect_identical(with_option("width", 50, format(tbl)), format(tbl, width = 50))
})

test_that("print() colors the console only when it can show colors", {
    tbl <- gw_parse_ir(load_fixture("minimal/colspan_basic.json"))
    capture.output(out <- withVisible(print(tbl)))
    expect_false(out$visible)
    expect_identical(out$value, tbl)
    # Under testthat stdout isn't a terminal: plain.
    plain <- capture.output(print(tbl))
    expect_identical(plain, format(tbl))
    expect_false(gw_use_color())
    colored <- with_option("gridwell.color", TRUE, capture.output(print(tbl)))
    expect_true(any(grepl("\x1b", colored, fixed = TRUE)))
    expect_identical(with_option("gridwell.color", FALSE, capture.output(print(tbl))), plain)
    # In a terminal: colors, unless NO_COLOR is set.
    local_mocked_bindings(gw_console_has_color = function() TRUE)
    old <- Sys.getenv("NO_COLOR", unset = NA)
    on.exit(if (is.na(old)) Sys.unsetenv("NO_COLOR") else Sys.setenv(NO_COLOR = old))
    Sys.unsetenv("NO_COLOR")
    expect_true(gw_use_color())
    expect_true(any(grepl("\x1b", capture.output(print(tbl)), fixed = TRUE)))
    Sys.setenv(NO_COLOR = "1")
    expect_false(gw_use_color())
    expect_identical(capture.output(print(tbl)), plain)
    expect_true(with_option("gridwell.color", TRUE, gw_use_color()))
    expect_false(with_option("gridwell.color", FALSE, {
        Sys.unsetenv("NO_COLOR")
        gw_use_color()
    }))
})

test_that("an invalid table prints its errors rather than failing", {
    tbl <- gw_parse_ir(load_fixture("invalid/col_count_mismatch.json"))
    lines <- format(tbl)
    expect_match(lines[1], "; invalid (1 error)>", fixed = TRUE)
    expect_identical(lines[-1], paste0("  ", gw_validate(tbl)))
    expect_identical(capture.output(print(tbl)), lines)
})

# ─── knitr ───

test_that("in an HTML document a table is raw HTML with an id of its own, left alone by Quarto", {
    skip_if_not_installed("knitr")
    tbl <- gw_parse_ir(load_fixture("minimal/colspan_basic.json"))
    set.seed(1)
    seed <- .Random.seed
    out <- with_knit_format("html", list(knitr::knit_print(tbl), knitr::knit_print(tbl)))
    expect_identical(.Random.seed, seed)
    ids <- character(0)
    for (o in out) {
        expect_s3_class(o, "knit_asis")
        expect_match(o, "^\n```\\{=html\\}\n<div id=\"gw-[0-9a-f]{10}\" class=\"gw_container\"")
        expect_match(o, "\n```\n$")
        id <- regmatches(o, regexpr("gw-[0-9a-f]{10}", o))
        expect_match(o, paste0(":where(#", id, ".gw_container)"), fixed = TRUE)
        html <- sub("\n```\n$", "", sub("^\n```\\{=html\\}\n", "", as.character(o)))
        expect_identical(html, gw_render_html(tbl, table_id = id, quarto_disable_processing = TRUE))
        expect_match(html, '<table class="gw_table" data-quarto-disable-processing="true">', fixed = TRUE)
        ids <- c(ids, id)
    }
    expect_false(ids[1] == ids[2])
    # Without pandoc (an `.Rhtml` file), the HTML itself.
    o <- with_knit_format(NULL, knitr::knit_print(tbl), out_format = "html")
    expect_match(o, "^\n<div id=\"gw-")
})

test_that("in a PDF document a table is raw LaTeX that asks for its packages", {
    skip_if_not_installed("knitr")
    skip_if_not_installed("rmarkdown")
    tbl <- gw_parse_ir(load_fixture("minimal/colspan_basic.json"))
    for (to in c("latex", "beamer")) {
        o <- with_knit_format(to, knitr::knit_print(tbl))
        expect_identical(as.character(o), paste0("\n```{=latex}\n", gw_render_latex(tbl), "\n```\n"))
        meta <- attr(o, "knit_meta")
        expect_true(all(vapply(meta, inherits, TRUE, "latex_dependency")))
        packages <- vapply(meta, `[[`, "", "name")
        expect_setequal(packages, c("booktabs", "multirow", "longtable", "tabularx", "xcolor", "ulem"))
        expect_identical(meta[[which(packages == "xcolor")]]$options, "table")
        expect_identical(meta[[which(packages == "ulem")]]$options, "normalem")
    }
    # An `.Rnw` file: the LaTeX itself.
    o <- with_knit_format(NULL, knitr::knit_print(tbl), out_format = "latex")
    expect_identical(as.character(o), paste0("\n", gw_render_latex(tbl), "\n"))
})

test_that("Typst gets Typst, and other formats the drawing as text", {
    skip_if_not_installed("knitr")
    tbl <- gw_parse_ir(load_fixture("minimal/colspan_basic.json"))
    o <- with_knit_format("typst", knitr::knit_print(tbl))
    expect_identical(as.character(o), paste0("\n```{=typst}\n", gw_render_typst(tbl), "\n```\n"))
    for (to in c("docx", "pptx", "rtf", "odt")) {
        o <- with_knit_format(to, knitr::knit_print(tbl))
        text <- sub("\n$", "", gw_render_ansi(tbl, color = FALSE))
        expect_identical(as.character(o), paste0("\n```\n", text, "\n```\n"))
    }
})

test_that("knitting a document shows the table", {
    skip_if_not_installed("knitr")
    dir <- tempfile("gw-knit-")
    dir.create(dir)
    json_path <- file.path(fixtures_dir, "minimal/colspan_basic.json")
    rmd <- file.path(dir, "doc.Rmd")
    writeLines(c(
        "```{r, echo = FALSE}",
        sprintf("tbl <- gridwell::gw_parse_ir(paste(readLines(%s), collapse = '\\n'))", deparse(json_path)),
        "tbl",
        "```"
    ), rmd)
    md <- knitr::knit(rmd, file.path(dir, "doc.md"), quiet = TRUE, envir = new.env())
    text <- paste(readLines(md), collapse = "\n")
    expect_match(text, "<div id=\"gw-[0-9a-f]{10}\" class=\"gw_container\"")
    expect_false(grepl("## <gw_table", text, fixed = TRUE))
})
