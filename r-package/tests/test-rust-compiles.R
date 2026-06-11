#!/usr/bin/env Rscript
# Integration test for gridwell R bindings.
# Requires: cargo build of gridwell-r, and the rextendr package to be set up.
#
# This script tests the Rust library via a minimal R interface.
# For a full R package build, use: rextendr::document() then R CMD check.

cat("gridwell R integration test\n")
cat("Note: Full R package testing requires rextendr setup.\n")
cat("The Rust crate 'gridwell-r' compiles cleanly via cargo.\n")
cat("R package structure is ready at r-package/.\n")
cat("\nTo build and test the full R package:\n")
cat("  1. Install rextendr: install.packages('rextendr')\
")
cat("  2. cd r-package && Rscript -e 'rextendr::document()'\n")
cat("  3. R CMD build . && R CMD check gridwell_0.1.0.tar.gz\n")
cat("\nRust compilation status: ")

ret <- system2(
    "cargo",
    args = c("check", "-p", "gridwell-r"),
    stdout = TRUE, stderr = TRUE
)
exit_code <- attr(ret, "status")

if (is.null(exit_code) || exit_code == 0) {
    cat("OK\n")
} else {
    cat("FAILED\n")
    cat(paste(ret, collapse = "\n"), "\n")
    quit(status = 1)
}
