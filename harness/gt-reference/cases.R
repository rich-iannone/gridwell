# The gt reference cases: each builds a gt table. Sourced by make.R (which writes
# the reference files) and by the R package's tests (which check that the
# checked-in IR still matches what gt_to_ir() produces).
#
# Adding a case: add it here, run make.R, and check in the new files.

# Base-R column subset (keeps the cases free of dplyr).
dplyr_free_select <- function(df, cols) df[, cols, drop = FALSE]

# ─── Cases ───
#
# `description` is shown in the gallery; `build` returns the gt table. Fixed ids
# keep gt's HTML deterministic (gt otherwise generates a random id for its scoped
# CSS).

demographics_data <- function() {
    data.frame(
        group = rep(c("Age (years)", "Sex, n (%)", "Race, n (%)"), times = c(3, 2, 3)),
        label = c(
            "n", "Mean (SD)", "Median (Min, Max)",
            "Female", "Male",
            "White", "Black or African American", "Other"
        ),
        placebo = c(
            "86", "75.2 (8.59)", "76.0 (52, 89)",
            "53 (61.6)", "33 (38.4)",
            "78 (90.7)", "8 (9.3)", "0"
        ),
        low = c(
            "84", "75.7 (8.29)", "77.5 (51, 88)",
            "50 (59.5)", "34 (40.5)",
            "78 (92.9)", "6 (7.1)", "0"
        ),
        high = c(
            "84", "74.4 (7.89)", "76.0 (56, 88)",
            "40 (47.6)", "44 (52.4)",
            "74 (88.1)", "9 (10.7)", "1 (1.2)"
        )
    )
}

cases <- list(
    demographics = list(
        description = paste(
            "Clinical demographics (t14_1_1): row groups, stub, a spanner,",
            "a footnote and a source note"
        ),
        build = function() {
            demographics_data() |>
                gt(rowname_col = "label", groupname_col = "group", id = "demographics") |>
                tab_header(
                    title = "Table 14.1.1 Summary of Demographic Characteristics",
                    subtitle = "Safety Population"
                ) |>
                tab_spanner(label = "Xanomeline", columns = c(low, high)) |>
                cols_label(placebo = "Placebo", low = "Low Dose", high = "High Dose") |>
                tab_footnote(
                    footnote = "Percentages are based on the number of subjects in each arm.",
                    locations = cells_row_groups(groups = "Sex, n (%)")
                ) |>
                tab_source_note("Source: ADSL (CDISC pilot study)")
        }
    ),
    exibble_basic = list(
        description = "gt(exibble) with no options: gt's default look for every column type",
        build = function() {
            gt(exibble, id = "exibble_basic")
        }
    ),
    groups_and_stub = list(
        description = "Row groups and a stub (exibble with rowname_col and groupname_col)",
        build = function() {
            exibble |>
                dplyr_free_select(c("row", "group", "char", "num", "currency")) |>
                gt(rowname_col = "row", groupname_col = "group", id = "groups_and_stub") |>
                fmt_number(columns = num, decimals = 2) |>
                fmt_currency(columns = currency, currency = "USD")
        }
    ),
    nested_spanners = list(
        description = "Two levels of spanners over formatted numbers",
        build = function() {
            head(mtcars[, c("mpg", "cyl", "disp", "hp", "qsec", "am", "gear")], 6) |>
                gt(rownames_to_stub = TRUE, id = "nested_spanners") |>
                tab_spanner(label = "Engine", columns = c(cyl, disp, hp), id = "engine") |>
                tab_spanner(label = "Drivetrain", columns = c(am, gear), id = "drivetrain") |>
                tab_spanner(label = "Specifications", spanners = c("engine", "drivetrain")) |>
                fmt_number(columns = c(mpg, qsec), decimals = 1)
        }
    ),
    summary_rows = list(
        description = "Group summary rows and a grand summary",
        build = function() {
            exibble |>
                dplyr_free_select(c("row", "group", "num", "currency")) |>
                gt(rowname_col = "row", groupname_col = "group", id = "summary_rows") |>
                fmt_number(columns = num, decimals = 1) |>
                summary_rows(
                    columns = c(num, currency),
                    fns = list(Total = ~ sum(., na.rm = TRUE)),
                    fmt = ~ fmt_number(., decimals = 1)
                ) |>
                grand_summary_rows(
                    columns = c(num, currency),
                    fns = list(`Grand total` = ~ sum(., na.rm = TRUE)),
                    fmt = ~ fmt_number(., decimals = 1)
                )
        }
    ),
    tab_style = list(
        description = "tab_style(): fills, bold, colored text, and a styled column label",
        build = function() {
            head(exibble[, c("char", "num", "currency")], 5) |>
                gt(id = "tab_style") |>
                tab_style(
                    style = list(cell_fill(color = "#E8F0FE"), cell_text(weight = "bold")),
                    locations = cells_body(columns = num, rows = num > 1)
                ) |>
                tab_style(
                    style = cell_text(color = "#B42318", style = "italic"),
                    locations = cells_body(columns = char)
                ) |>
                tab_style(
                    style = cell_fill(color = "#F2F4F7"),
                    locations = cells_column_labels()
                )
        }
    ),
    striped_theme = list(
        description = "opt_stylize() theme with row striping",
        build = function() {
            head(exibble[, c("char", "fctr", "num", "date")], 6) |>
                gt(id = "striped_theme") |>
                tab_header(title = "A stylized table") |>
                opt_stylize(style = 6, color = "blue") |>
                opt_row_striping()
        }
    ),
    notes_and_marks = list(
        description = "Title, subtitle, footnotes on several locations, and source notes",
        build = function() {
            head(exibble[, c("char", "num", "date")], 4) |>
                gt(id = "notes_and_marks") |>
                tab_header(title = md("**Notes** and marks"), subtitle = "Subtitle text") |>
                tab_footnote("A column label note.", locations = cells_column_labels(columns = num)) |>
                tab_footnote("A body cell note.", locations = cells_body(columns = char, rows = 2)) |>
                tab_footnote("A title note.", locations = cells_title(groups = "title")) |>
                tab_source_note("Source note one.") |>
                tab_source_note(md("Source note *two*."))
        }
    )
)
