skip_if_not_installed("gt")
skip_if_not_installed("jsonlite")

# The IR of a gt table, parsed and validated.
ir_of <- function(gt_tbl) {
    ir <- gt_to_ir_list(gt_tbl)
    tbl <- gw_parse_ir(as.character(gt_to_ir(gt_tbl)))
    expect_equal(gw_validate(tbl), character(0))
    list(ir = ir, tbl = tbl)
}

texts <- function(nodes) vapply(nodes, function(n) n$value %||% "", character(1))
`%||%` <- function(a, b) if (is.null(a)) b else a

body_cell <- function(ir, row, col) ir$table$tbody[[1]]$rows[[row]]$cells[[col]]

test_that("cell values are gt's formatted values, with typed values as shown", {
    df <- data.frame(x = c("a", "b"), v = c(76, 0.718317324737925))
    out <- ir_of(gt::gt(df) |> gt::fmt_number(v, decimals = 1))
    expect_equal(texts(body_cell(out$ir, 1, 2)$content), "76.0")
    expect_equal(texts(body_cell(out$ir, 2, 2)$content), "0.7")
    expect_equal(body_cell(out$ir, 2, 2)$typed_value, list(type = "number", value = 0.7))
    expect_equal(body_cell(out$ir, 1, 1)$typed_value$type, "string")
})

test_that("md() labels become bold and italic runs", {
    df <- data.frame(a = 1, b = 2)
    tbl <- gt::gt(df) |>
        gt::cols_label(a = gt::md("**Drug A**  \nN = 98"), b = gt::md("*p*-value")) |>
        gt::tab_header(title = gt::md("**Table 1**"))
    ir <- ir_of(tbl)$ir
    labels <- ir$table$thead$rows[[1]]$cells
    a <- labels[[1]]$content
    expect_equal(vapply(a, `[[`, "", "type"), c("styled_text", "line_break", "text"))
    expect_equal(texts(a), c("Drug A", "", "N = 98"))
    expect_equal(ir$styles$defs[[a[[1]]$style_id]], list(font_weight = "bold"))
    b <- labels[[2]]$content
    expect_equal(texts(b), c("p", "-value"))
    expect_equal(ir$styles$defs[[b[[1]]$style_id]], list(font_style = "italic"))
    expect_equal(ir$column_spec[[1]]$label, "Drug A N = 98")
    expect_equal(texts(ir$header$title$content), "Table 1")
    expect_false(any(grepl("*", unlist(ir$table$thead), fixed = TRUE)))
})

test_that("spanners become a spanner row with placeholders", {
    df <- data.frame(name = "x", q1 = 1, q2 = 2, total = 3)
    ir <- ir_of(gt::gt(df) |> gt::tab_spanner("Scores", columns = c(q1, q2)))$ir
    expect_equal(ir$config$header_rows, 2L)
    spanner <- ir$table$thead$rows[[1]]
    expect_equal(spanner$role, "spanner_label")
    expect_equal(vapply(spanner$cells, `[[`, 1L, "colspan"), c(1L, 2L, 1L, 1L))
    expect_equal(vapply(spanner$cells, `[[`, NA, "is_placeholder"), c(FALSE, FALSE, TRUE, FALSE))
    expect_equal(texts(spanner$cells[[2]]$content), "Scores")
})

test_that("footnote marks are placed in the cells they annotate", {
    df <- data.frame(x = c("a", "b"), v = c(1, 2))
    tbl <- gt::gt(df) |>
        gt::tab_footnote("Column note", locations = gt::cells_column_labels(columns = x)) |>
        gt::tab_footnote("Cell note", locations = gt::cells_body(columns = x, rows = 2))
    ir <- ir_of(tbl)$ir
    marks <- function(nodes) {
        Filter(function(n) identical(n$type, "footnote_mark"), nodes)
    }
    label_mark <- marks(ir$table$thead$rows[[1]]$cells[[1]]$content)
    cell_mark <- marks(body_cell(ir, 2, 1)$content)
    expect_length(label_mark, 1)
    expect_length(cell_mark, 1)
    expect_length(marks(body_cell(ir, 1, 1)$content), 0)
    notes <- ir$footer$footnotes
    expect_equal(vapply(notes, `[[`, "", "mark"), c("1", "2"))
    expect_equal(texts(notes[[1]]$content), "Column note")
    expect_equal(label_mark[[1]]$ref, notes[[1]]$id)
    expect_equal(cell_mark[[1]]$ref, notes[[2]]$id)
    # Text output shows marks as superscripts, not as digits run into the value.
    ansi <- gw_render_ansi(gw_parse_ir(as.character(gt_to_ir(tbl))))
    expect_true(grepl("x\u00b9", enc2utf8(ansi)))
})

test_that("tab_style() styles become IR styles on their cells", {
    df <- data.frame(x = c("a", "b"), v = c(1, 2))
    tbl <- gt::gt(df) |>
        gt::tab_style(
            gt::cell_text(weight = "bold"),
            locations = gt::cells_body(columns = x, rows = 2)
        ) |>
        gt::tab_style(gt::cell_fill(color = "#EEEEEE"), locations = gt::cells_body(rows = 2))
    ir <- ir_of(tbl)$ir
    expect_null(body_cell(ir, 1, 1)$style_id)
    style <- ir$styles$defs[[body_cell(ir, 2, 1)$style_id]]
    expect_equal(style$font_weight, "bold")
    expect_equal(style$background_color, "#EEEEEE")
    expect_equal(ir$styles$defs[[body_cell(ir, 2, 2)$style_id]]$background_color, "#EEEEEE")
})

test_that("row groups keep gt's display order", {
    df <- data.frame(g = c("b", "a", "b"), x = c("r1", "r2", "r3"), v = 1:3)
    ir <- ir_of(gt::gt(df, groupname_col = "g", rowname_col = "x"))$ir
    groups <- ir$table$tbody
    expect_equal(vapply(groups, `[[`, "", "group_id"), c("b", "a"))
    expect_equal(
        texts(lapply(groups[[1]]$rows, function(r) r$cells[[1]]$content[[1]])),
        c("r1", "r3")
    )
    expect_true(groups[[1]]$rows[[1]]$cells[[1]]$is_stub)
})

test_that("a gtsummary table converts with its formatting", {
    skip_if_not_installed("gtsummary")
    # add_p() tidies its test results with broom (only suggested by gtsummary).
    skip_if_not_installed("broom", "1.0.8")
    tbl <- gtsummary::trial |>
        gtsummary::tbl_summary(by = trt, include = c(age, grade)) |>
        gtsummary::add_p() |>
        gtsummary::bold_labels() |>
        gtsummary::modify_spanning_header(gtsummary::all_stat_cols() ~ "**Treatment**") |>
        gtsummary::as_gt()
    out <- ir_of(tbl)
    json <- as.character(gt_to_ir(tbl))
    expect_false(grepl("**", json, fixed = TRUE))
    expect_false(grepl("0.7183", json, fixed = TRUE))
    expect_length(out$ir$footer$footnotes, 2)
    expect_equal(out$ir$config$header_rows, 2L)
    # The variable labels are bold (bold_labels()), the levels aren't.
    rows <- out$ir$table$tbody[[1]]$rows
    styled <- vapply(rows, function(r) !is.null(r$cells[[1]]$style_id), logical(1))
    expect_equal(sum(styled), 2)
    for (fmt in c("rtf", "ansi", "latex", "html")) expect_gt(nchar(gw_render(out$tbl, fmt)), 0)
    for (fmt in c("docx", "xlsx", "pptx")) expect_gt(length(gw_render_binary(out$tbl, fmt)), 0)
})

test_that("the gt reference IR in harness/gt-reference is up to date", {
    # The "gt vs gridwell" gallery page renders checked-in IR; if the emitter's
    # output changes, regenerate it with harness/gt-reference/make.R. Only in a
    # source checkout, and only with the gt version the references were made with
    # (another gt can format values differently). Installed packages and CRAN have
    # no harness/ directory, so this never runs there.
    ref_dir <- file.path(testthat::test_path(), "..", "..", "..", "harness", "gt-reference")
    skip_if_not(dir.exists(ref_dir), "not in a source checkout")
    manifest <- jsonlite::fromJSON(file.path(ref_dir, "manifest.json"), simplifyVector = FALSE)
    skip_if_not(
        identical(as.character(utils::packageVersion("gt")), manifest$gt_version),
        paste("references made with gt", manifest$gt_version)
    )
    env <- new.env()
    suppressPackageStartupMessages(library(gt))
    sys.source(file.path(ref_dir, "cases.R"), envir = env)
    expect_setequal(names(env$cases), vapply(manifest$cases, `[[`, "", "name"))
    for (name in names(env$cases)) {
        ir <- as.character(gt_to_ir(env$cases[[name]]$build()))
        stored <- paste(readLines(file.path(ref_dir, paste0(name, ".ir.json")), encoding = "UTF-8"),
                        collapse = "\n")
        expect_identical(ir, stored, label = paste(name, "IR (regenerate with make.R)"))
    }
})

# ---- Summary rows, table options, missing values ----

test_that("group and grand summary rows are emitted as gt formats them", {
    tbl <- exibble[, c("row", "group", "num")] |>
        gt::gt(rowname_col = "row", groupname_col = "group") |>
        gt::summary_rows(
            columns = num, fns = list(Total = ~ sum(., na.rm = TRUE)),
            fmt = ~ gt::fmt_number(., decimals = 1)
        ) |>
        gt::grand_summary_rows(
            columns = num, fns = list(`Grand total` = ~ sum(., na.rm = TRUE)),
            fmt = ~ gt::fmt_number(., decimals = 1)
        )
    ir <- gt_to_ir_list(tbl)
    text <- function(cell) paste(vapply(cell$content, `[[`, "", "value"), collapse = "")
    groups <- ir$table$tbody
    expect_length(groups, 3)
    for (g in groups[1:2]) {
        expect_length(g$summary_rows, 1)
        r <- g$summary_rows[[1]]
        expect_identical(r$role, "summary_row")
        expect_identical(text(r$cells[[1]]), "Total")
        expect_true(r$cells[[1]]$is_stub)
    }
    expect_identical(text(groups[[1]]$summary_rows[[1]]$cells[[2]]), "480.1")
    grand <- groups[[3]]
    expect_null(grand$label)
    expect_length(grand$rows, 0)
    expect_identical(grand$summary_rows[[1]]$role, "grand_summary_row")
    expect_identical(text(grand$summary_rows[[1]]$cells[[1]]), "Grand total")
    expect_identical(text(grand$summary_rows[[1]]$cells[[2]]), "9,663,030.1")
    expect_length(gw_validate(gw_parse_ir(gt_to_ir(tbl))), 0)
})

test_that("a grand summary without a stub gets one, empty in the data rows", {
    tbl <- exibble[1:3, c("char", "num")] |>
        gt::gt() |>
        gt::grand_summary_rows(columns = num, fns = list(Sum = ~ sum(.)))
    ir <- gt_to_ir_list(tbl)
    expect_identical(ir$config$stub_cols, 1L)
    expect_identical(ir$column_spec[[1]]$id, "::rowname::")
    expect_length(ir$column_spec, 3)
    body <- ir$table$tbody[[1]]$rows
    expect_true(all(vapply(body, function(r) r$cells[[1]]$is_stub, TRUE)))
    expect_true(all(vapply(body, function(r) length(r$cells[[1]]$content) == 0 ||
        identical(r$cells[[1]]$content[[1]]$value, ""), TRUE)))
    expect_length(gw_validate(gw_parse_ir(gt_to_ir(tbl))), 0)
})

test_that("tables without summaries are unchanged by the summary support", {
    ir <- gt_to_ir_list(gt::gt(exibble[1:2, c("char", "num")]))
    expect_identical(ir$config$stub_cols, 0L)
    expect_length(ir$table$tbody, 1)
    expect_length(ir$table$tbody[[1]]$summary_rows, 0)
})

test_that("changed table options become IR styles; defaults add nothing", {
    plain <- gt_to_ir_list(gt::gt(exibble[1:4, c("char", "num")]))
    expect_length(plain$styles$defs, 0)
    expect_false(plain$config$row_striping)
    expect_length(plain$styles$conditionals, 0)

    tbl <- exibble[1:4, c("row", "char", "num")] |>
        gt::gt(rowname_col = "row") |>
        gt::tab_options(
            column_labels.background.color = "#0076BA",
            column_labels.font.weight = "bold",
            stub.background.color = "#89D3FE",
            table_body.hlines.style = "none"
        )
    ir <- gt_to_ir_list(tbl)
    defs <- ir$styles$defs
    head_style <- defs[[ir$table$thead$rows[[1]]$style_id]]
    expect_identical(head_style$background_color, "#0076BA")
    expect_identical(head_style$color, "#FFFFFF") # gt's contrast choice
    expect_identical(head_style$font_weight, "bold")
    row1 <- ir$table$tbody[[1]]$rows[[1]]
    expect_identical(defs[[row1$style_id]]$border$top$style, "none")
    stub_style <- defs[[row1$cells[[1]]$style_id]]
    expect_identical(stub_style$background_color, "#89D3FE")
    expect_null(row1$cells[[2]]$style_id)
    expect_length(gw_validate(gw_parse_ir(gt_to_ir(tbl))), 0)
})

test_that("tab_style() on a stub cell overrides the stub option", {
    tbl <- exibble[1:2, c("row", "num")] |>
        gt::gt(rowname_col = "row") |>
        gt::tab_options(stub.background.color = "#89D3FE") |>
        gt::tab_style(gt::cell_fill("#FF0000"), gt::cells_stub(rows = 1))
    ir <- gt_to_ir_list(tbl)
    defs <- ir$styles$defs
    rows <- ir$table$tbody[[1]]$rows
    expect_identical(defs[[rows[[1]]$cells[[1]]$style_id]]$background_color, "#FF0000")
    expect_identical(defs[[rows[[2]]$cells[[1]]$style_id]]$background_color, "#89D3FE")
})

test_that("row striping: gt's default color is the IR's; a custom one is conditional", {
    default <- gt_to_ir_list(gt::opt_row_striping(gt::gt(exibble[1:4, c("char", "num")])))
    expect_true(default$config$row_striping)
    expect_true(default$config$row_striping_include_body)
    expect_length(default$styles$conditionals, 0)

    custom <- exibble[1:4, c("row", "num")] |>
        gt::gt(rowname_col = "row") |>
        gt::opt_row_striping() |>
        gt::tab_options(row.striping.background_color = "#EDF7FC")
    ir <- gt_to_ir_list(custom)
    expect_false(ir$config$row_striping)
    cond <- ir$styles$conditionals[[1]]
    expect_identical(cond$selector$row_parity, "even")
    expect_identical(cond$selector$scope, "tbody")
    expect_identical(cond$style$background_color, "#EDF7FC")
    # gt doesn't stripe the stub by default, so stub cells keep the table background.
    stub <- ir$styles$defs[[ir$table$tbody[[1]]$rows[[2]]$cells[[1]]$style_id]]
    expect_identical(stub$background_color, "#FFFFFF")
})

test_that("missing values show as gt shows them and sort as missing", {
    ir <- gt_to_ir_list(gt::gt(exibble[5:6, c("char", "num")]))
    rows <- ir$table$tbody[[1]]$rows
    na_char <- rows[[1]]$cells[[1]]
    expect_identical(na_char$content[[1]]$value, "NA")
    expect_null(na_char$sort_key)
    expect_null(na_char$typed_value)
    expect_identical(rows[[1]]$cells[[2]]$sort_key, tolower(rows[[1]]$cells[[2]]$content[[1]]$value))
})
