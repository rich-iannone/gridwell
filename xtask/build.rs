//! Writes the docs' Rust blocks to `OUT_DIR/rust_snippets.rs` for
//! `tests/rust_snippets.rs`; see `snippets.rs`.

#[allow(dead_code)]
#[path = "snippets.rs"]
mod snippets;

use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    println!("cargo:rerun-if-changed=snippets.rs");
    println!("cargo:rerun-if-changed={}", root.join("docs").display());
    let mut all = Vec::new();
    for page in snippets::pages(root) {
        let path = root.join(&page);
        println!("cargo:rerun-if-changed={}", path.display());
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
            .replace("\r\n", "\n");
        let page = page.to_string_lossy().replace('\\', "/");
        all.extend(snippets::snippets(&page, &text));
    }
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("rust_snippets.rs");
    std::fs::write(out, snippets::generate(&all)).unwrap();
}
