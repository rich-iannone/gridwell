# ─── Parse tests ───

test_that("gw_parse_ir works with valid JSON", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    expect_s3_class(tbl, "gw_table")
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

test_that("gw_to_json keeps a node of an unknown type", {
    ir <- jsonlite::fromJSON(load_fixture("minimal/minimal_1x1.json"), simplifyVector = FALSE)
    future <- list(type = "sparkline", values = list(1L, 2.5), options = list(a = list(TRUE)))
    cell <- ir$table$tbody[[1]]$rows[[1]]$cells[[1]]
    cell$content <- c(cell$content, list(future))
    ir$table$tbody[[1]]$rows[[1]]$cells[[1]] <- cell
    tbl <- gw_parse_ir(jsonlite::toJSON(ir, auto_unbox = TRUE, null = "null", digits = NA))
    expect_length(gw_validate(tbl), 0)
    back <- jsonlite::fromJSON(gw_to_json(tbl), simplifyVector = FALSE)
    content <- back$table$tbody[[1]]$rows[[1]]$cells[[1]]$content
    # Every field kept (a JSON object has no key order).
    expect_setequal(names(content[[2]]), names(future))
    expect_identical(content[[2]][names(future)], future)
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
    # Without the default look's rules: a different workbook.
    bare <- gw_render_xlsx(tbl, base_styles = FALSE)
    expect_equal(bare[1:4], charToRaw("PK\x03\x04"))
    expect_false(identical(bare, data))
})

test_that("gw_render_pptx produces ZIP bytes", {
    json <- load_fixture("minimal/minimal_1x1.json")
    tbl <- gw_parse_ir(json)
    data <- gw_render_pptx(tbl)
    expect_true(is.raw(data))
    expect_equal(data[1:4], charToRaw("PK\x03\x04"))
    # Without the default look's rules: a different slide.
    bare <- gw_render_pptx(tbl, base_styles = FALSE)
    expect_equal(bare[1:4], charToRaw("PK\x03\x04"))
    expect_false(identical(bare, data))
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

test_that("gw_warnings names what a format can't express", {
    tbl <- gw_parse_ir(load_fixture("minimal/styles_borders.json"))
    latex <- gw_warnings(tbl, "latex")
    expect_length(latex, 1)
    expect_true(startsWith(
        latex,
        "[UNSUPPORTED_FEATURE] latex can't express border-styles (group 1 row 2, column 1"
    ))
    expect_identical(gw_warnings(tbl, " LaTeX "), latex)
    expect_identical(gw_warnings(tbl, "html"), character(0))
    # Every format, in registry order.
    every <- gw_warnings(tbl)
    expect_identical(every, gw_warnings(tbl, format = NULL))
    expect_identical(vapply(strsplit(every, " "), `[`, "", 2), c("latex", "typst"))
    expect_identical(every[1], latex)
    # A warning doesn't stop rendering.
    expect_true(nzchar(gw_render_latex(tbl)))
    expect_identical(gw_warnings(gw_parse_ir(load_fixture("minimal/minimal_1x1.json"))), character(0))
})

test_that("gw_warnings checks the format and the table", {
    tbl <- gw_parse_ir(load_fixture("minimal/styles_borders.json"))
    expect_error(gw_warnings(tbl, "pdf"), "unknown format \"pdf\"", fixed = TRUE)
    expect_error(gw_warnings(tbl, 1), "`format` must be NULL or a single string", fixed = TRUE)
    invalid <- gw_parse_ir(load_fixture("invalid/span_overlap.json"))
    expect_error(gw_warnings(invalid, "latex"), "[SPAN_OVERLAP]", fixed = TRUE)
    expect_error(gw_warnings(invalid), "[SPAN_OVERLAP]", fixed = TRUE)
})

# A fixture at another IR version.
versioned <- function(version, fixture = "minimal/minimal_1x1.json") {
    sub('"ir_version": "1.0"', sprintf('"ir_version": "%s"', version), load_fixture(fixture),
        fixed = TRUE)
}

newer <- paste0(
    "[NEWER_IR_VERSION] the table is IR 1.3, newer than the IR 1.0 this gridwell reads: ",
    "anything a newer IR version added is left out (a newer gridwell may render it)"
)

test_that("a newer minor IR version is read, with a warning", {
    expect_warning(tbl <- gw_parse_ir(versioned("1.3")), newer, fixed = TRUE)
    expect_identical(gw_validate(tbl), character(0))
    expect_true(nzchar(gw_render_html(tbl)))
    # First of the table's warnings, for any format.
    expect_identical(gw_warnings(tbl), newer)
    expect_identical(gw_warnings(tbl, "html"), newer)
    borders <- versioned("1.3", "minimal/styles_borders.json")
    expect_warning(tbl <- gw_parse_ir(borders), newer, fixed = TRUE)
    w <- gw_warnings(tbl, "latex")
    expect_length(w, 2)
    expect_identical(w[1], newer)
    expect_true(startsWith(w[2], "[UNSUPPORTED_FEATURE] latex"))
    # Once per parse: a table given as IR is parsed, and warns.
    expect_warning(as_gw_table(versioned("1.3")), newer, fixed = TRUE)
    expect_warning(gw_render_html(versioned("1.3")), newer, fixed = TRUE)
    # Not again when a table read back with readRDS() is parsed again.
    path <- tempfile(fileext = ".rds")
    saveRDS(suppressWarnings(gw_parse_ir(versioned("1.3"))), path)
    expect_no_warning(gw_render_html(readRDS(path)))
    for (version in c("1.0", "01.00", "2.0")) {
        expect_no_warning(gw_parse_ir(versioned(version)))
    }
})

test_that("an invalid newer minor IR version table says its version", {
    json <- sub('"table_cols": 1', '"table_cols": 2', versioned("1.3"), fixed = TRUE)
    tbl <- suppressWarnings(gw_parse_ir(json))
    expect_error(gw_render_html(tbl), "[COL_COUNT]", fixed = TRUE)
    expect_error(
        gw_render_html(tbl),
        paste0(
            "(the table is IR 1.3, newer than the IR 1.0 this gridwell reads: ",
            "an error may be something a newer IR version added)"
        ),
        fixed = TRUE
    )
})

test_that("another major IR version is refused", {
    tbl <- gw_parse_ir(load_fixture("invalid/ir_version_unknown_major.json"))
    expect_identical(
        gw_validate(tbl),
        paste0(
            "[IR_VERSION] ir_version is \"2.0\", IR 2, and this gridwell ",
            "reads only IR 1.x (a newer gridwell may read it)"
        )
    )
    expect_error(gw_render_html(tbl), "[IR_VERSION]", fixed = TRUE)
    expect_match(gw_validate(gw_parse_ir(versioned("1"))), "not MAJOR.MINOR", fixed = TRUE)
    # IR 2 that doesn't parse as IR 1: the version is why.
    ir2 <- sub('"column_spec"', '"columns"', versioned("2.0"), fixed = TRUE)
    expect_error(
        gw_parse_ir(ir2),
        "Failed to parse IR: ir_version is \"2.0\", IR 2, and this gridwell reads only IR 1.x",
        fixed = TRUE
    )
    expect_error(
        gw_parse_ir(sub('"column_spec"', '"columns"', load_fixture("minimal/minimal_1x1.json"),
                        fixed = TRUE)),
        "missing field `column_spec`",
        fixed = TRUE
    )
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
    expect_type(f$experimental, "logical")
    expect_identical(f$name[f$experimental], c("svg", "ansi", "pandoc", "quarto"))
    html_opts <- jsonlite::fromJSON(f$options[f$name == "html"])
    expect_equal(html_opts$class_prefix, "gw")
    for (opts in f$options) {
        expect_true(jsonlite::fromJSON(opts)$raw)
    }
})

test_that("raw = FALSE writes no raw markup", {
    ir <- jsonlite::fromJSON(load_fixture("minimal/minimal_1x1.json"), simplifyVector = FALSE)
    ir$table$tbody[[1]]$rows[[1]]$cells[[1]]$content <- list(
        list(type = "raw", format = "html", value = "<b>hi</b><script>x</script>"),
        list(type = "raw", format = "latex", value = "\\input{secret}")
    )
    tbl <- gw_parse_ir(jsonlite::toJSON(ir, auto_unbox = TRUE, null = "null"))
    expect_match(gw_render_html(tbl), "<script>", fixed = TRUE)
    expect_match(gw_render_latex(tbl), "\\input", fixed = TRUE)
    html <- gw_render_html(tbl, raw = FALSE)
    expect_false(grepl("<script>", html, fixed = TRUE))
    expect_match(html, ">hi</span>", fixed = TRUE)
    expect_identical(html, gw_render(tbl, "html", list(raw = FALSE)))
    expect_false(grepl("\\input", gw_render_latex(tbl, raw = FALSE), fixed = TRUE))
    expect_gt(length(gw_render_docx(tbl, raw = FALSE)), 0)
    expect_error(gw_render_html(tbl, raw = "no"), "raw")
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

# ─── gw_save ───

save_extensions <- c(
    html = "html", tex = "latex", typ = "typst", rtf = "rtf", svg = "svg",
    docx = "docx", xlsx = "xlsx", pptx = "pptx"
)

read_file_bytes <- function(path) readBin(path, "raw", file.size(path))

# A fresh, empty directory (inside the session's tempdir(), removed with it).
local_dir <- function() {
    dir <- tempfile("gw-save-")
    dir.create(dir)
    dir
}

test_that("gw_save infers every format and writes the rendered bytes", {
    tbl <- gw_parse_ir(load_fixture("comprehensive/reference_table.json"))
    kinds <- stats::setNames(gw_formats()$kind, gw_formats()$name)
    dir <- local_dir()
    for (ext in names(save_extensions)) {
        fmt <- save_extensions[[ext]]
        path <- file.path(dir, paste0("t.", ext))
        gw_save(tbl, path)
        expected <- if (kinds[[fmt]] == "text") {
            charToRaw(enc2utf8(gw_render(tbl, fmt)))
        } else {
            gw_render_binary(tbl, fmt)
        }
        expect_identical(read_file_bytes(path), expected, label = fmt)
    }
})

test_that("gw_save returns the path invisibly", {
    tbl <- gw_parse_ir(load_fixture("minimal/minimal_1x1.json"))
    path <- file.path(local_dir(), "t.RTF")
    expect_invisible(gw_save(tbl, path))
    expect_identical(gw_save(tbl, path), path)
    expect_identical(read_file_bytes(path), charToRaw(enc2utf8(gw_render_rtf(tbl))))
})

test_that("gw_save takes an explicit format, which overrides the extension", {
    tbl <- gw_parse_ir(load_fixture("comprehensive/reference_table.json"))
    dir <- local_dir()
    for (fmt in c("pandoc", "quarto")) {
        path <- file.path(dir, paste0(fmt, ".json"))
        gw_save(tbl, path, format = fmt)
        expect_identical(read_file_bytes(path), charToRaw(enc2utf8(gw_render(tbl, fmt))))
    }
    path <- file.path(dir, "no-extension")
    gw_save(tbl, path, format = "LaTeX")
    expect_identical(read_file_bytes(path), charToRaw(enc2utf8(gw_render_latex(tbl))))
})

test_that("gw_save writes .txt as plain text unless color = TRUE", {
    tbl <- gw_parse_ir(load_fixture("comprehensive/reference_table.json"))
    path <- file.path(local_dir(), "t.txt")
    gw_save(tbl, path)
    bytes <- read_file_bytes(path)
    expect_false(any(bytes == as.raw(0x1b)))
    expect_identical(bytes, charToRaw(enc2utf8(gw_render_ansi(tbl, color = FALSE))))
    gw_save(tbl, path, color = TRUE)
    expect_identical(read_file_bytes(path), charToRaw(enc2utf8(gw_render_ansi(tbl))))
    expect_true(grepl("\x1b", gw_render_ansi(tbl), fixed = TRUE))
})

test_that("gw_save passes writer options through", {
    skip_if_not_installed("jsonlite")
    tbl <- gw_parse_ir(load_fixture("comprehensive/reference_table.json"))
    path <- file.path(local_dir(), "t.tex")
    gw_save(tbl, path, booktabs = FALSE)
    expect_identical(
        read_file_bytes(path),
        charToRaw(enc2utf8(gw_render_latex(tbl, booktabs = FALSE)))
    )
})

test_that("gw_save errors say what is wrong and write nothing", {
    skip_if_not_installed("jsonlite")
    tbl <- gw_parse_ir(load_fixture("minimal/minimal_1x1.json"))
    invalid <- gw_parse_ir(load_fixture("invalid/col_count_mismatch.json"))
    dir <- local_dir()
    p <- function(f) file.path(dir, f)
    expect_error(gw_save(tbl, p("t.json")), "pandoc or quarto.*`format =`")
    expect_error(gw_save(tbl, p("t.pdf")), "no format writes \"\\.pdf\" files")
    expect_error(gw_save(tbl, p("t")), "no file extension")
    expect_error(gw_save(tbl, p("t.html"), format = "nope"), "unknown format")
    expect_error(gw_save(tbl, p("t.html"), nope = 1), "unknown field")
    expect_error(gw_save(invalid, p("t.docx")), "COL_COUNT")
    expect_error(gw_save(tbl, p(file.path("missing-dir", "t.html"))), "cannot write.*missing-dir")
    expect_error(gw_save(tbl, c("a.html", "b.html")), "single file path")
    expect_error(gw_save(tbl, NA_character_), "single file path")
    expect_error(gw_save(tbl, p("t.html"), format = 1), "single format name")
    expect_error(gw_save(42, p("t.html")), "gw_parse_ir")
    expect_error(gw_save("not a table", p("t.html")), "Failed to parse IR")
    expect_length(list.files(dir), 0)
})

test_that("gw_save accepts a gt table", {
    skip_if_not_installed("gt")
    skip_if_not_installed("jsonlite")
    gt_tbl <- gt::gt(head(mtcars, 3)) |> gt::tab_header(title = "Cars")
    path <- file.path(local_dir(), "t.rtf")
    expect_identical(gt_tbl |> gw_save(path), path)
    expected <- gw_render_rtf(gw_parse_ir(gt_to_ir(gt_tbl)))
    expect_identical(read_file_bytes(path), charToRaw(enc2utf8(expected)))
    expect_match(expected, "Cars", fixed = TRUE)
})

test_that("gw_save writes .html as a fragment unless standalone = TRUE", {
    skip_if_not_installed("jsonlite")
    tbl <- gw_parse_ir(load_fixture("comprehensive/reference_table.json"))
    path <- file.path(local_dir(), "t.html")
    gw_save(tbl, path)
    fragment <- rawToChar(read_file_bytes(path))
    Encoding(fragment) <- "UTF-8"
    expect_identical(read_file_bytes(path), charToRaw(enc2utf8(gw_render_html(tbl))))
    expect_false(startsWith(fragment, "<!doctype"))
    gw_save(tbl, path, standalone = TRUE)
    doc <- rawToChar(read_file_bytes(path))
    Encoding(doc) <- "UTF-8"
    expect_true(startsWith(doc, "<!doctype html>"))
    expect_identical(
        read_file_bytes(path),
        charToRaw(enc2utf8(gw_render_html(tbl, standalone = TRUE)))
    )
    expect_true(grepl(fragment, doc, fixed = TRUE))
})
