fixtures_dir <- normalizePath(
    file.path(testthat::test_path(), "..", "..", "..", "fixtures"),
    mustWork = TRUE
)

load_fixture <- function(rel_path) {
    readLines(file.path(fixtures_dir, rel_path), warn = FALSE) |>
        paste(collapse = "\n")
}

# ─── Parse tests ───

test_that("gw_parse_ir works with valid JSON", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    expect_true(is(tbl, "externalptr"))
})

test_that("gw_parse_ir errors on invalid JSON", {
    expect_error(gw_parse_ir("{ not valid }"), "parse|Parse")
})

# ─── Validate tests ───

test_that("gw_validate returns empty for valid table", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    errors <- gw_validate(tbl)
    expect_equal(length(errors), 0)
})

test_that("gw_validate returns errors for invalid table", {
    json <- load_fixture("invalid/col_count_mismatch.json")
    tbl <- gw_parse_ir(json)
    errors <- gw_validate(tbl)
    expect_gt(length(errors), 0)
})

# ─── Serialize test ───

test_that("gw_to_json round-trips", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    output <- gw_to_json(tbl)
    parsed <- jsonlite::fromJSON(output, simplifyVector = FALSE)
    expect_equal(parsed$ir_version, "1.0")
})

# ─── Text render tests ───

test_that("gw_render_html produces HTML", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    html <- gw_render_html(tbl)
    expect_true(grepl("<table", html))
})

test_that("gw_render_latex produces LaTeX", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    latex <- gw_render_latex(tbl)
    expect_true(grepl("\\\\begin\\{", latex))
})

test_that("gw_render_typst produces Typst", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    typst <- gw_render_typst(tbl)
    expect_true(nchar(typst) > 0)
})

test_that("gw_render_rtf produces RTF", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    rtf <- gw_render_rtf(tbl)
    expect_true(grepl("^\\{\\\\rtf1", rtf))
})

test_that("gw_render_svg produces SVG", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    svg <- gw_render_svg(tbl)
    expect_true(grepl("<svg", svg))
})

test_that("gw_render_ansi produces output", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    ansi <- gw_render_ansi(tbl)
    expect_true(nchar(ansi) > 0)
})

test_that("gw_render_pandoc produces output", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    pandoc <- gw_render_pandoc(tbl)
    expect_true(nchar(pandoc) > 0)
})

test_that("gw_render_quarto produces output", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    quarto <- gw_render_quarto(tbl)
    expect_true(nchar(quarto) > 0)
})

test_that("gw_render dispatches by format name", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    html <- gw_render(tbl, "html")
    expect_true(grepl("<table", html))
})

test_that("gw_render errors on unknown format", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    expect_error(gw_render(tbl, "nope"), 'unknown format "nope"', fixed = TRUE)
})

# ─── Binary render tests ───

test_that("gw_render_docx produces ZIP bytes", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    data <- gw_render_docx(tbl)
    expect_true(is.raw(data))
    expect_equal(data[1:4], charToRaw("PK\x03\x04"))
})

test_that("gw_render_xlsx produces ZIP bytes", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    data <- gw_render_xlsx(tbl)
    expect_true(is.raw(data))
    expect_equal(data[1:4], charToRaw("PK\x03\x04"))
})

test_that("gw_render_pptx produces ZIP bytes", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    data <- gw_render_pptx(tbl)
    expect_true(is.raw(data))
    expect_equal(data[1:4], charToRaw("PK\x03\x04"))
})

test_that("gw_render_binary dispatches by format", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    data <- gw_render_binary(tbl, "docx")
    expect_true(is.raw(data))
    expect_equal(data[1:4], charToRaw("PK\x03\x04"))
})

test_that("gw_render_binary errors on unknown format", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    expect_error(gw_render_binary(tbl, "pdf"), 'unknown format "pdf"', fixed = TRUE)
})

# ─── Rendering refuses invalid IR ───

text_renderers <- list(
    gw_render_html, gw_render_latex, gw_render_typst, gw_render_rtf,
    gw_render_svg, gw_render_ansi, gw_render_pandoc, gw_render_quarto
)
binary_renderers <- list(gw_render_docx, gw_render_xlsx, gw_render_pptx)

test_that("every renderer refuses invalid IR with the validation errors", {
    tbl <- gw_parse_ir(load_fixture("invalid/span_overflow_right.json"))
    for (render in c(text_renderers, binary_renderers)) {
        expect_error(render(tbl), "failed validation")
        expect_error(render(tbl), "[SPAN_OVERFLOW_RIGHT]", fixed = TRUE)
    }
})

test_that("gw_render and gw_render_binary refuse invalid IR", {
    tbl <- gw_parse_ir(load_fixture("invalid/col_count_mismatch.json"))
    for (fmt in c("html", "latex", "typst", "rtf", "svg", "ansi", "pandoc", "quarto")) {
        expect_error(gw_render(tbl, fmt), "[COL_COUNT]", fixed = TRUE)
    }
    for (fmt in c("docx", "xlsx", "pptx")) {
        expect_error(gw_render_binary(tbl, fmt), "[COL_COUNT]", fixed = TRUE)
    }
})

test_that("gw_validate reports documented rule ids", {
    tbl <- gw_parse_ir(load_fixture("invalid/span_overlap.json"))
    errors <- gw_validate(tbl)
    expect_true(any(startsWith(errors, "[SPAN_OVERLAP]")))
})

test_that("adjacent colspans render in every format (regression)", {
    # Valid IR that used to panic the RTF/SVG writers and drop cells elsewhere.
    cell <- function(text, colspan = 1) {
        sprintf('{"content":[{"type":"text","value":"%s"}],"colspan":%d}', text, colspan)
    }
    ph <- '{"content":[],"is_placeholder":true}'
    cells <- paste(c(cell("A", 2), ph, cell("B", 2), ph, cell("C"), cell("D")), collapse = ",")
    spec <- paste(sprintf('{"id":"c%d"}', 0:5), collapse = ",")
    json <- sprintf(
        paste0(
            '{"ir_version":"1.0","config":{"table_cols":6,"header_rows":0,"body_rows":1},',
            '"styles":{"defs":{},"compositions":{},"conditionals":[]},',
            '"column_spec":[%s],',
            '"table":{"thead":{"rows":[]},"tbody":[{"rows":[{"cells":[%s]}]}]}}'
        ),
        spec, cells
    )
    tbl <- gw_parse_ir(json)
    expect_equal(length(gw_validate(tbl)), 0)
    for (render in text_renderers) {
        out <- render(tbl)
        for (label in c("A", "B", "C", "D")) expect_true(grepl(label, out, fixed = TRUE))
    }
    for (render in binary_renderers) {
        expect_gt(length(render(tbl)), 0)
    }
})

# ─── Format registry and writer options ───

test_that("gw_formats lists every format with its options", {
    f <- gw_formats()
    expect_s3_class(f, "data.frame")
    expect_equal(
        f$name,
        c("html", "latex", "typst", "rtf", "svg", "ansi", "pandoc", "quarto",
          "docx", "xlsx", "pptx")
    )
    expect_equal(f$kind[f$name == "docx"], "binary")
    expect_equal(f$extension[f$name == "quarto"], "json")
    html_opts <- jsonlite::fromJSON(f$options[f$name == "html"])
    expect_equal(html_opts$class_prefix, "gw")
})

test_that("options work as a list, as named arguments, and as JSON", {
    tbl <- gw_parse_ir(load_fixture("comprehensive/reference_table.json"))
    default <- gw_render(tbl, "latex")
    as_list <- gw_render(tbl, "latex", list(booktabs = FALSE))
    as_args <- gw_render_latex(tbl, booktabs = FALSE)
    as_json <- gw_render(tbl, "latex", '{"booktabs": false}')
    expect_identical(as_list, as_args)
    expect_identical(as_list, as_json)
    expect_false(identical(as_list, default))
    expect_false(grepl("\\toprule", as_list))
    expect_identical(gw_render(tbl, "latex", NULL), default)
    expect_identical(gw_render(tbl, "latex", list()), default)
})

test_that("bad options are errors that say what is wrong", {
    tbl <- gw_parse_ir(load_fixture("minimal/minimal_1x1.json"))
    expect_error(gw_render_html(tbl, inline_style = TRUE), "unknown field")
    expect_error(gw_render(tbl, "html", list(TRUE)), "named list")
    expect_error(gw_render(tbl, "html", 42), "NULL, a named list, or a JSON string")
    expect_error(gw_render(tbl, "html", "{oops"), "not valid JSON")
    expect_error(gw_render_svg(tbl, font_size = -1), "font_size")
    expect_error(gw_render_html(tbl, class_prefix = "x\"><script>"), "class_prefix")
})

test_that("text and binary entry points reject the other kind", {
    tbl <- gw_parse_ir(load_fixture("minimal/minimal_1x1.json"))
    expect_error(gw_render(tbl, "docx"), "binary format")
    expect_error(gw_render_binary(tbl, "html"), "text format")
    expect_match(gw_render(tbl, "HTML"), "^<div")
})
