# Build the gt reference set for the "gt vs gridwell" gallery page.
#
# For each case in cases.R this writes, into this directory:
#   <name>.gt.html   gt's own HTML (`as_raw_html(inline_css = FALSE)`), the look
#                    gridwell's HTML is measured against
#   <name>.ir.json   the same table as gridwell IR (`gridwell::gt_to_ir()`)
# and manifest.json (case descriptions plus the R, gt and gridwell versions used).
#
# The files are checked in so the visual harness (which has no R) can rasterize
# both sides. Regenerate after changing the gt emitter or upgrading gt, from the
# repository root, with the development gridwell installed:
#
#   R CMD INSTALL r-package
#   Rscript harness/gt-reference/make.R
#
# Then run `cargo xtask gallery` and open harness/gallery/gt.html.

suppressPackageStartupMessages({
    library(gt)
    library(gridwell)
})

args <- commandArgs(trailingOnly = FALSE)
script <- sub("^--file=", "", args[grepl("^--file=", args)])
out_dir <- if (length(script) == 1) dirname(normalizePath(script)) else "harness/gt-reference"

source(file.path(out_dir, "cases.R"))

# ─── Write ───

manifest_cases <- list()
for (name in names(cases)) {
    case <- cases[[name]]
    tbl <- case$build()
    html <- as_raw_html(tbl, inline_css = FALSE)
    writeLines(enc2utf8(html), file.path(out_dir, paste0(name, ".gt.html")), useBytes = TRUE)
    ir <- gt_to_ir(tbl)
    writeLines(enc2utf8(as.character(ir)), file.path(out_dir, paste0(name, ".ir.json")), useBytes = TRUE)
    manifest_cases[[length(manifest_cases) + 1]] <- list(name = name, description = case$description)
    message("wrote ", name)
}

manifest <- list(
    generated_by = "harness/gt-reference/make.R",
    r_version = paste(R.version$major, R.version$minor, sep = "."),
    gt_version = as.character(utils::packageVersion("gt")),
    gridwell_version = as.character(utils::packageVersion("gridwell")),
    cases = manifest_cases
)
writeLines(
    jsonlite::toJSON(manifest, auto_unbox = TRUE, pretty = TRUE),
    file.path(out_dir, "manifest.json")
)
message("wrote manifest.json (", length(cases), " cases)")
