//! Compile `tests/c/smoke.c` against `include/gridwell.h` and the freshly built
//! `cdylib`, then run it. This tests the C API the way C callers use it: the real
//! header, the real shared library, and C-side allocation and freeing.
//!
//! Variants: strict C99, C++ (the header's `cpp_compat`), and C99 with
//! AddressSanitizer + UBSan (double frees, bad frees, overruns on the C side).
//!
//! If no C compiler is available the tests are skipped, unless
//! `GRIDWELL_REQUIRE_C_TOOLCHAIN` is set (as in CI), in which case they fail.

mod common;

use common::{build_shared_lib, compiler, crate_dir, skip_or_fail};
use std::process::Command;

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
