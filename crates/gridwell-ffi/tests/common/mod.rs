//! Helpers for the tests that compile C against the header and the freshly built
//! shared library: `c_api` (smoke.c) and `docs` (the C API page's example).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Build the `cdylib` and return its directory. `cargo test` only builds this crate as
/// an rlib for the test binary, so without this the C program would link against
/// whatever (possibly stale) shared library a previous `cargo build` left behind.
///
/// The build uses the test's own target directory (the parent of
/// `CARGO_TARGET_TMPDIR`), so it reuses the test build's dependencies, and the
/// library's path is the one cargo reports: where cargo puts build outputs is up to
/// cargo (Rust 1.100 moved test binaries out of `target/<profile>/deps/`).
pub fn build_shared_lib() -> Result<PathBuf, String> {
    static BUILT: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            let target_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).parent().unwrap();
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
            let mut cmd = Command::new(cargo);
            cmd.args([
                "build",
                "--quiet",
                "-p",
                "gridwell-ffi",
                "--lib",
                "--message-format=json-render-diagnostics",
                "--target-dir",
            ])
            .arg(target_dir);
            if !cfg!(debug_assertions) {
                cmd.arg("--release");
            }
            let out = cmd.output().map_err(|e| format!("cannot run cargo: {e}"))?;
            if !out.status.success() {
                return Err(format!(
                    "cargo build failed:\n{}",
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
            let stdout = String::from_utf8_lossy(&out.stdout);
            let lib = shared_lib(&stdout).ok_or_else(|| {
                format!("cargo built no gridwell_ffi shared library:\n{stdout}")
            })?;
            Ok(lib.parent().unwrap().to_path_buf())
        })
        .clone()
}

/// The shared library among the artifacts in cargo's JSON messages, if it exists.
fn shared_lib(messages: &str) -> Option<PathBuf> {
    let name = if cfg!(target_os = "macos") {
        "libgridwell_ffi.dylib"
    } else if cfg!(target_os = "linux") {
        "libgridwell_ffi.so"
    } else {
        return None;
    };
    messages
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|m| m["reason"] == "compiler-artifact" && m["target"]["name"] == "gridwell_ffi")
        .flat_map(|m| {
            m["filenames"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|f| f.as_str().map(PathBuf::from))
                .collect::<Vec<_>>()
        })
        .find(|f| f.file_name().is_some_and(|n| n == name) && f.exists())
}

pub fn compiler(var: &str, default: &str) -> Option<String> {
    let cc = std::env::var(var).unwrap_or_else(|_| default.to_string());
    let ok = Command::new(&cc)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    ok.then_some(cc)
}

pub fn skip_or_fail(reason: &str) {
    if std::env::var_os("GRIDWELL_REQUIRE_C_TOOLCHAIN").is_some_and(|v| !v.is_empty()) {
        panic!("C API test cannot run: {reason}");
    }
    eprintln!("skipping C API test: {reason}");
}
