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
    expect_error(gw_render(tbl, "nope"), "Unknown format")
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
    expect_error(gw_render_binary(tbl, "pdf"), "Unknown binary format")
})
