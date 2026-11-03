# Showing a table: as text in the console, and as the document's own format in
# R Markdown and Quarto.

#' Format or print a gridwell table.
#'
#' A table prints as a description (its size, row groups and title) and the
#' table drawn as text, as [gw_render_ansi()] draws it, fitted to the console's
#' width. The console gets colors unless it can't show them, `NO_COLOR` is set,
#' or `options(gridwell.color = FALSE)`; `options(gridwell.color = TRUE)` forces
#' them. An invalid table prints its validation errors instead.
#'
#' @param x A `gw_table`.
#' @param width The widest the drawing may be, in characters (`0` for no limit).
#' @param color Whether to use ANSI colors and styles.
#' @param ... Unused.
#' @return `format()` gives a character vector of lines; `print()` returns `x`,
#'   invisibly.
#' @export
format.gw_table <- function(x, width = getOption("width"), color = FALSE, ...) {
    ptr <- gw_ptr(x)
    header <- paste0("<gw_table: ", rs_describe(ptr), ">")
    errors <- rs_validate(ptr)
    if (length(errors) > 0) {
        return(c(header, paste0("  ", errors)))
    }
    options <- sprintf(
        '{"color": %s, "true_color": %s, "max_width": %d}',
        tolower(isTRUE(color)), tolower(gw_true_color()), as.integer(width)
    )
    text <- rs_render_text(ptr, "ansi", options)
    c(header, "", strsplit(sub("\n$", "", text), "\n", fixed = TRUE)[[1]])
}

#' @rdname format.gw_table
#' @export
print.gw_table <- function(x, ...) {
    cat(format(x, color = gw_use_color(), ...), sep = "\n")
    invisible(x)
}

# Whether the console shows ANSI colors: `options(gridwell.color =)` if set, else
# not with `NO_COLOR` (https://no-color.org), else RStudio's console or a
# terminal.
gw_use_color <- function() {
    option <- getOption("gridwell.color")
    if (!is.null(option)) {
        return(isTRUE(option))
    }
    if (nzchar(Sys.getenv("NO_COLOR"))) {
        return(FALSE)
    }
    gw_console_has_color()
}

# Whether the console is one that shows colors: RStudio's, or a terminal.
gw_console_has_color <- function() {
    if (identical(Sys.getenv("RSTUDIO"), "1") && interactive()) {
        return(TRUE)
    }
    isatty(stdout()) && !identical(Sys.getenv("TERM"), "dumb")
}

# 24-bit colors where the terminal says it has them; 256 otherwise.
gw_true_color <- function() {
    Sys.getenv("COLORTERM") %in% c("truecolor", "24bit")
}

# The LaTeX packages the latex writer's output needs.
gw_latex_packages <- function() {
    list(
        rmarkdown::latex_dependency("booktabs"),
        rmarkdown::latex_dependency("multirow"),
        rmarkdown::latex_dependency("longtable"),
        rmarkdown::latex_dependency("tabularx"),
        rmarkdown::latex_dependency("xcolor", options = "table"),
        rmarkdown::latex_dependency("ulem", options = "normalem")
    )
}

#' Show a gridwell table in R Markdown or Quarto.
#'
#' knitr calls this for a table a chunk returns. It's written in the format of
#' the document: HTML for HTML output (with an id of its own, so its styles
#' apply to it alone, and marked so Quarto doesn't restyle it), LaTeX for PDF output (asking for the LaTeX packages it
#' needs), Typst for Typst output. Other formats (Word, PowerPoint, RTF) get the
#' table drawn as text.
#'
#' @param x A `gw_table`.
#' @param ... Unused.
#' @return Output for knitr, from [knitr::asis_output()].
#' @exportS3Method knitr::knit_print
knit_print.gw_table <- function(x, ...) {
    ptr <- gw_ptr(x)
    to <- knitr::pandoc_to()
    # Without pandoc (`.Rnw`, `.Rhtml`) the output is the format itself;
    # through pandoc, a raw block of it.
    raw <- function(text, format) {
        if (is.null(to)) {
            paste0("\n", text, "\n")
        } else {
            paste0("\n```{=", format, "}\n", text, "\n```\n")
        }
    }
    if (knitr::is_latex_output()) {
        meta <- if (requireNamespace("rmarkdown", quietly = TRUE)) gw_latex_packages()
        latex <- rs_render_text(ptr, "latex", NULL)
        return(knitr::asis_output(raw(latex, "latex"), meta = meta))
    }
    if (identical(to, "typst")) {
        return(knitr::asis_output(raw(rs_render_text(ptr, "typst", NULL), "typst")))
    }
    if (knitr::is_html_output()) {
        return(knitr::asis_output(raw(rs_display_html(ptr), "html")))
    }
    text <- rs_render_text(ptr, "ansi", '{"color": false}')
    knitr::asis_output(paste0("\n```\n", sub("\n$", "", text), "\n```\n"))
}
