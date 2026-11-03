# The IR fixtures: a copy in tests/testthat/fixtures in a source package (`cargo
# xtask r-source` bundles one), or else the repository's fixtures/ in a checkout.
fixtures_dir <- local({
    bundled <- testthat::test_path("fixtures")
    checkout <- file.path(testthat::test_path(), "..", "..", "..", "fixtures")
    normalizePath(if (dir.exists(bundled)) bundled else checkout, mustWork = TRUE)
})

load_fixture <- function(rel_path) {
    readLines(file.path(fixtures_dir, rel_path), warn = FALSE) |>
        paste(collapse = "\n")
}
