//! Helpers for the tests that compile C against the header and the freshly built
//! shared library: `c_api` (smoke.c) and `docs` (the C API page's example).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// `target/<profile>`: the test binary lives in `target/<profile>/deps/`.
fn profile_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    exe.parent().unwrap().parent().unwrap().to_path_buf()
}

/// Build the `cdylib` and return its directory. `cargo test` only builds this crate as
/// an rlib for the test binary, so without this the C program would link against
/// whatever (possibly stale) shared library a previous `cargo build` left behind.
pub fn build_shared_lib() -> Result<PathBuf, String> {
    static BUILT: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            let profile_dir = profile_dir();
            let target_dir = profile_dir.parent().unwrap();
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
            let mut cmd = Command::new(cargo);
            cmd.args([
                "build",
                "--quiet",
                "-p",
                "gridwell-ffi",
                "--lib",
                "--target-dir",
            ])
            .arg(target_dir);
            if profile_dir.file_name().is_some_and(|n| n == "release") {
                cmd.arg("--release");
            }
            let out = cmd.output().map_err(|e| format!("cannot run cargo: {e}"))?;
            if !out.status.success() {
                return Err(format!(
                    "cargo build failed:\n{}",
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
            match shared_lib(&profile_dir) {
                Some(_) => Ok(profile_dir),
                None => Err(format!(
                    "no gridwell_ffi shared library in {}",
                    profile_dir.display()
                )),
            }
        })
        .clone()
}

fn shared_lib(dir: &Path) -> Option<PathBuf> {
    let name = if cfg!(target_os = "macos") {
        "libgridwell_ffi.dylib"
    } else if cfg!(target_os = "linux") {
        "libgridwell_ffi.so"
    } else {
        return None;
    };
    let path = dir.join(name);
    path.exists().then_some(path)
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
