# Build the gt reference set for the "gt vs gridwell" gallery page.
#
# For each case in conformance/cases.R this writes, into this directory:
#   <name>.gt.html   gt's own HTML (`as_raw_html(inline_css = FALSE)`), the look
#                    gridwell's HTML is measured against
#   <name>.ir.json   the same table as gridwell IR (`gridwell::gt_to_ir()`: gt's own
#                    `as_ir()` when the installed gt has it, else gridwell's copy)
# and manifest.json (case descriptions, the R, gt and gridwell versions used, and
# which emitter wrote the IR).
#
# The files are checked in so the visual harness (which has no R) can rasterize
# both sides. Regenerate after changing the gt emitter or upgrading gt, from the
# repository root, with the development gridwell installed:
#
#   R CMD INSTALL r-package
#   Rscript harness/gt-reference/make.R
#
# Then run `python conformance/make.py`, and `cargo xtask gallery` and open
# harness/gallery/gt.html.

suppressPackageStartupMessages({
    library(gt)
    library(gridwell)
})

args <- commandArgs(trailingOnly = FALSE)
script <- sub("^--file=", "", args[grepl("^--file=", args)])
out_dir <- if (length(script) == 1) dirname(normalizePath(script)) else "harness/gt-reference"

source(file.path(out_dir, "..", "..", "conformance", "cases.R"))

# ─── Write ───

# gt's own emitter when there is one, so the references follow it.
upstream <- !is.null(gridwell:::.gw_upstream_emitter())
manifest_cases <- list()
for (name in names(cases)) {
    case <- cases[[name]]
    tbl <- case$build()
    html <- as_raw_html(tbl, inline_css = FALSE)
    writeLines(enc2utf8(html), file.path(out_dir, paste0(name, ".gt.html")), useBytes = TRUE)
    ir <- gt_to_ir(tbl, emitter = if (upstream) "upstream" else "gridwell")
    writeLines(enc2utf8(as.character(ir)), file.path(out_dir, paste0(name, ".ir.json")), useBytes = TRUE)
    manifest_cases[[length(manifest_cases) + 1]] <- list(name = name, description = case$description)
    message("wrote ", name)
}

manifest <- list(
    generated_by = "harness/gt-reference/make.R",
    r_version = paste(R.version$major, R.version$minor, sep = "."),
    gt_version = as.character(utils::packageVersion("gt")),
    gridwell_version = as.character(utils::packageVersion("gridwell")),
    ir_emitter = if (upstream) "gt" else "gridwell",
    cases = manifest_cases
)
writeLines(
    jsonlite::toJSON(manifest, auto_unbox = TRUE, pretty = TRUE),
    file.path(out_dir, "manifest.json")
)
message("wrote manifest.json (", length(cases), " cases)")
