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
