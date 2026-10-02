//! The committed C header must match what cbindgen generates from the current source.
//!
//! Regenerate after changing the FFI:
//!     GRIDWELL_BLESS=1 cargo test -p gridwell-ffi --test header

use std::path::Path;

fn generate() -> String {
    let crate_dir = env!("CARGO_MANIFEST_DIR");
    let config = cbindgen::Config::from_file(Path::new(crate_dir).join("cbindgen.toml"))
        .expect("read cbindgen.toml");
    let mut out = Vec::new();
    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_config(config)
        .generate()
        .expect("cbindgen failed")
        .write(&mut out);
    // Normalize to LF: on Windows checkouts cbindgen.toml has CRLF line endings, and
    // its multi-line `header` string carries the `\r`s into the output.
    String::from_utf8(out)
        .expect("header is UTF-8")
        .replace('\r', "")
}

#[test]
fn header_is_up_to_date() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("include/gridwell.h");
    let generated = generate();

    if std::env::var_os("GRIDWELL_BLESS").is_some() {
        std::fs::write(&path, &generated).expect("write header");
        return;
    }

    let committed = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace('\r', "");
    if committed != generated {
        let first_diff = committed
            .lines()
            .zip(generated.lines())
            .enumerate()
            .find(|(_, (c, g))| c != g)
            .map(|(i, (c, g))| {
                format!(
                    "first difference at line {}:\n  committed: {c:?}\n  generated: {g:?}",
                    i + 1
                )
            })
            .unwrap_or_else(|| {
                format!(
                    "lengths differ: committed {} lines, generated {} lines",
                    committed.lines().count(),
                    generated.lines().count()
                )
            });
        panic!(
            "include/gridwell.h is stale ({first_diff}).\nRegenerate with:\n    \
             GRIDWELL_BLESS=1 cargo test -p gridwell-ffi --test header"
        );
    }
}

#[test]
fn header_declares_the_whole_api() {
    let h = generate();
    for name in [
        "gridwell_parse_ir",
        "gridwell_validate",
        "gridwell_render_text",
        "gridwell_render_binary",
        "gridwell_free_table",
        "gridwell_free_text_result",
        "gridwell_free_binary_result",
        "gridwell_free_error",
        "gridwell_error_message",
        "gridwell_error_code",
        "GRIDWELL_ERR_PARSE",
        "GRIDWELL_ERR_VALIDATE",
        "GRIDWELL_ERR_RENDER",
        "GRIDWELL_ERR_INVALID_ARG",
        "GRIDWELL_ERR_PANIC",
        "typedef struct GridwellTable GridwellTable",
        "typedef struct GridwellError GridwellError",
    ] {
        assert!(h.contains(name), "header is missing {name}");
    }
    // Internal helpers must not leak into the C API.
    for name in ["guard", "render_args", "make_error", "set_error"] {
        assert!(
            !h.contains(&format!(" {name}(")),
            "header exposes internal {name}"
        );
    }
}
