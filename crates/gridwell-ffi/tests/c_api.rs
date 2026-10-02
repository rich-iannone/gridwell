//! Compile `tests/c/smoke.c` against `include/gridwell.h` and the freshly built
//! `cdylib`, then run it. This tests the C API the way C callers use it: the real
//! header, the real shared library, and C-side allocation and freeing.
//!
//! Variants: strict C99, C++ (the header's `cpp_compat`), and C99 with
//! AddressSanitizer + UBSan (double frees, bad frees, overruns on the C side).
//!
//! If no C compiler is available the tests are skipped, unless
//! `GRIDWELL_REQUIRE_C_TOOLCHAIN` is set (as in CI), in which case they fail.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

fn crate_dir() -> PathBuf {
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
fn build_shared_lib() -> Result<PathBuf, String> {
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

fn compiler(var: &str, default: &str) -> Option<String> {
    let cc = std::env::var(var).unwrap_or_else(|_| default.to_string());
    let ok = Command::new(&cc)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    ok.then_some(cc)
}

fn skip_or_fail(reason: &str) {
    if std::env::var_os("GRIDWELL_REQUIRE_C_TOOLCHAIN").is_some_and(|v| !v.is_empty()) {
        panic!("C API test cannot run: {reason}");
    }
    eprintln!("skipping C API test: {reason}");
}

/// Compile smoke.c with `compiler` + `flags`, run it, and assert it passes.
fn build_and_run(name: &str, compiler_var: &str, default: &str, flags: &[&str]) {
    if cfg!(windows) {
        return skip_or_fail("not supported on Windows yet");
    }
    let Some(cc) = compiler(compiler_var, default) else {
        return skip_or_fail(&format!("no working {default} (set {compiler_var})"));
    };
    // A failed build is a real failure, not a missing toolchain.
    let lib_dir = build_shared_lib().unwrap_or_else(|e| panic!("{name}: {e}"));

    let out_dir =
        std::env::temp_dir().join(format!("gridwell-c-api-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&out_dir).unwrap();
    let exe = out_dir.join("smoke");

    let compile = Command::new(&cc)
        .args(flags)
        .arg("-I")
        .arg(crate_dir().join("include"))
        .arg(crate_dir().join("tests/c/smoke.c"))
        .arg("-L")
        .arg(&lib_dir)
        .arg("-lgridwell_ffi")
        .arg(format!("-Wl,-rpath,{}", lib_dir.display()))
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{name}: compilation failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let fixtures = crate_dir().join("../../fixtures");
    let run = Command::new(&exe)
        .arg(&fixtures)
        // Sanitizer reports must fail the run, not just print.
        .env("ASAN_OPTIONS", "halt_on_error=1:detect_leaks=0")
        .env("UBSAN_OPTIONS", "halt_on_error=1:print_stacktrace=1")
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{name}: smoke test failed (status {:?}):\nstdout:\n{}\nstderr:\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "ok");
}

#[test]
fn c99_strict() {
    build_and_run(
        "c99",
        "CC",
        "cc",
        &[
            "-x",
            "c",
            "-std=c99",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pedantic",
        ],
    );
}

#[test]
fn cpp_compat() {
    build_and_run(
        "cpp",
        "CXX",
        "c++",
        &["-x", "c++", "-std=c++11", "-Wall", "-Wextra", "-Werror"],
    );
}

#[test]
fn c99_with_sanitizers() {
    build_and_run(
        "asan",
        "CC",
        "cc",
        &[
            "-x",
            "c",
            "-std=c99",
            "-g",
            "-fsanitize=address,undefined",
            "-fno-omit-frame-pointer",
            "-fno-sanitize-recover=all",
        ],
    );
}
