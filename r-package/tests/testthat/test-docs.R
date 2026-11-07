# The docs' R blocks run.
#
# Every ```r block on a docs page (docs/**/*.qmd) or a README runs, the blocks of a
# page in order and sharing one environment, as a document's chunks would. Where an
# expression is followed by `#>` lines, they must be what it prints. The environment
# starts with what the pages take as given: `json` (the getting-started table's IR),
# `tbl` (that table), `tables` and `paths` (two of it, and where to save them) and,
# with gt installed, `gt_tbl` (the conformance kit's exibble_basic case). Without gt,
# a page with a block that uses it is skipped, as its other blocks may build on it;
# an expression calling a package that isn't installed (`purrr::walk2()`) is skipped.
# The blocks run in a scratch directory holding the getting-started table as
# `table.json` and the repository's `conformance/expected/`.
#
# `<!-- docs-check: skip -->` on the line before a fence (blank lines aside) leaves
# the block out, for code that can't run here (such as installing).
#
# The docs aren't in a source package, so outside the repository this is skipped.

docs_root <- normalizePath(
    file.path(testthat::test_path(), "..", "..", ".."),
    mustWork = FALSE
)
# In the repository only: an installed or checked package's tests are elsewhere.
docs_here <- identical(
    normalizePath(file.path(testthat::test_path(), "..", ".."), mustWork = FALSE),
    normalizePath(file.path(docs_root, "r-package"), mustWork = FALSE)
) && dir.exists(file.path(docs_root, "docs"))
docs_skip <- "<!-- docs-check: skip -->"
docs_readmes <- c(
    "README.md", "crates/gridwell/README.md", "crates/gridwell-cli/README.md",
    "conformance/README.md"
)
docs_needs_gt <- "\\b(gt|gt_tbl|exibble)\\b"

docs_pages <- function() {
    qmd <- list.files(file.path(docs_root, "docs"), pattern = "\\.qmd$", recursive = TRUE)
    qmd <- qmd[!grepl("(^|/)(_site|\\.quarto)/", qmd)]
    sort(c(file.path("docs", qmd), docs_readmes))
}

# The page's R blocks: the line of each opening fence and the code's lines.
docs_r_blocks <- function(lines) {
    out <- list()
    open <- NULL
    skip_next <- FALSE
    for (i in seq_along(lines)) {
        t <- sub("\\s+$", "", lines[[i]])
        if (is.null(open)) {
            if (startsWith(t, "```")) {
                info <- sub("^`+", "", t)
                ticks <- nchar(t) - nchar(info)
                info <- trimws(info)
                keep <- (info == "r" || grepl("^\\{\\.r[ }]", info)) && !skip_next
                open <- list(line = i, ticks = ticks, keep = keep, code = character())
                skip_next <- FALSE
            } else if (trimws(t) == docs_skip) {
                skip_next <- TRUE
            } else if (nzchar(trimws(t))) {
                skip_next <- FALSE
            }
        } else if (nchar(t) >= open$ticks && grepl("^`+$", t)) {
            if (open$keep) out[[length(out) + 1]] <- list(line = open$line, code = open$code)
            open <- NULL
        } else {
            open$code <- c(open$code, lines[[i]])
        }
    }
    out
}

# The packages an expression calls with `pkg::f` or `pkg:::f`.
docs_namespaces <- function(expr) {
    if (!is.call(expr)) return(character())
    head <- expr[[1]]
    if (identical(head, as.name("::")) || identical(head, as.name(":::"))) {
        return(as.character(expr[[2]]))
    }
    unlist(lapply(as.list(expr), docs_namespaces))
}

# The `#>` lines right after line `last` of `code`, or NULL.
docs_shown <- function(code, last) {
    rest <- code[-seq_len(last)]
    n <- match(FALSE, startsWith(rest, "#>"), nomatch = length(rest) + 1) - 1
    if (n == 0) return(NULL)
    sub("\\s+$", "", sub("^#> ?", "", rest[seq_len(n)]))
}

# Run the page's blocks in `env`; returns the failures.
docs_run_page <- function(page, lines, env) {
    failures <- character()
    for (block in docs_r_blocks(lines)) {
        exprs <- tryCatch(
            parse(text = block$code, keep.source = TRUE),
            error = function(e) e
        )
        if (inherits(exprs, "error")) {
            failures <- c(failures, sprintf("%s:%d: %s", page, block$line, conditionMessage(exprs)))
            next
        }
        refs <- attr(exprs, "srcref")
        for (k in seq_along(exprs)) {
            where <- sprintf("%s:%d", page, block$line + refs[[k]][1])
            pkgs <- docs_namespaces(exprs[[k]])
            if (!all(vapply(pkgs, requireNamespace, logical(1), quietly = TRUE))) next
            result <- tryCatch(withVisible(eval(exprs[[k]], env)), error = function(e) e)
            if (inherits(result, "error")) {
                failures <- c(failures, sprintf("%s: %s", where, conditionMessage(result)))
                break
            }
            shown <- docs_shown(block$code, refs[[k]][3])
            if (!is.null(shown)) {
                printed <- if (result$visible) utils::capture.output(print(result$value)) else character()
                printed <- sub("\\s+$", "", printed)
                if (!identical(printed, shown)) {
                    failures <- c(failures, sprintf(
                        "%s prints:\n%s\nbut the page shows:\n%s", where,
                        paste(printed, collapse = "\n"), paste(shown, collapse = "\n")
                    ))
                }
            }
        }
    }
    failures
}

docs_env <- function() {
    env <- new.env(parent = globalenv())
    env$json <- paste(readLines(file.path(docs_root, "docs", "examples", "hello.json"), warn = FALSE), collapse = "\n")
    env$tbl <- gw_parse_ir(env$json)
    env$tables <- list(env$tbl, env$tbl)
    env$paths <- c("one.html", "two.tex")
    if (requireNamespace("gt", quietly = TRUE)) env$gt_tbl <- gt::gt(gt::exibble)
    env
}

docs_read <- function(page) readLines(file.path(docs_root, page), warn = FALSE, encoding = "UTF-8")

# Run the page's blocks in a fresh scratch directory (base R: withr isn't a
# dependency).
docs_run_in_scratch <- function(page, lines) {
    dir <- tempfile("gridwell-docs-")
    dir.create(file.path(dir, "conformance"), recursive = TRUE)
    on.exit(unlink(dir, recursive = TRUE), add = TRUE)
    file.copy(file.path(docs_root, "conformance", "expected"), file.path(dir, "conformance"), recursive = TRUE)
    file.copy(file.path(docs_root, "docs", "examples", "hello.json"), file.path(dir, "table.json"))
    old <- setwd(dir)
    on.exit(setwd(old), add = TRUE, after = FALSE)
    op <- options(gridwell.color = FALSE, width = 80)
    on.exit(options(op), add = TRUE, after = FALSE)
    docs_run_page(page, lines, docs_env())
}

test_that("the docs' R blocks run", {
    skip_if_not(docs_here, "no docs here")
    has_gt <- requireNamespace("gt", quietly = TRUE)
    count <- 0
    for (page in docs_pages()) {
        lines <- docs_read(page)
        blocks <- docs_r_blocks(lines)
        count <- count + length(blocks)
        if (length(blocks) == 0) next
        if (!has_gt && any(vapply(blocks, function(b) any(grepl(docs_needs_gt, b$code, perl = TRUE)), logical(1)))) next
        failures <- docs_run_in_scratch(page, lines)
        expect(length(failures) == 0, paste(failures, collapse = "\n"))
    }
    # The pages hold this many; fewer means the extraction lost some.
    expect_gte(count, 20)
})

test_that("the docs check catches wrong blocks", {
    skip_if_not(docs_here, "no docs here")
    run <- function(text) docs_run_in_scratch("p.qmd", strsplit(text, "\n")[[1]])
    right <- paste(
        "```r", "x <- gw_render(tbl, \"latex\")", "```", "",
        "```{.r filename=\"a.R\"}", "stopifnot(startsWith(x, \"\\\\begin\"))", "```", "",
        "```r", "1 + 1", "#> [1] 2", "invisible(3)", "```",
        sep = "\n"
    )
    expect_equal(vapply(docs_r_blocks(strsplit(right, "\n")[[1]]), `[[`, 1L, "line"), c(1, 5, 9))
    expect_equal(run(right), character())

    wrong <- list(
        "an error" = "```r\ngw_render(tbl, \"htm\")\n```",
        "a name" = "```r\nprint(undefined)\n```",
        "a failed check" = "```r\nstopifnot(length(gw_validate(tbl)) == 1)\n```",
        "another output" = "```r\n1 + 1\n#> [1] 3\n```",
        "an invisible value" = "```r\ninvisible(2)\n#> [1] 2\n```",
        "a later block" = "```r\ny <- 1\n```\n\n```r\ny\n#> [1] 2\n```",
        "unparsable code" = "```r\nf(\n```"
    )
    for (what in names(wrong)) {
        expect_length(run(wrong[[what]]), 1)
    }
    # The failure gives the page's line; an error stops the block.
    failures <- run("Text.\n\n```r\n\nstop(\"boom\")\nstop(\"again\")\n```")
    expect_equal(failures, "p.qmd:5: boom")

    left_out <- paste0(
        docs_skip, "\n\n```r\nstop(\"skipped\")\n```\n\n",
        "```bash\nexit 1\n```\n\n",
        "````markdown\n```r\nstop(\"inside\")\n```\n````\n\n",
        "```r\nnotapkg1234::f()\n```"
    )
    expect_equal(run(left_out), character())
    # The marker covers one fence only.
    expect_length(docs_r_blocks(c(docs_skip, "", "Text.", "", "```r", "x", "```")), 1)
})
