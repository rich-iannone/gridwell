//! Every Rust block in the docs compiles (`build.rs` makes each a function; see
//! `snippets.rs`) and runs without error, in a scratch directory holding the
//! getting-started table as `table.json`. This file has one test, as it changes
//! the working directory.

#[allow(dead_code)]
#[path = "../snippets.rs"]
mod snippets;

include!(concat!(env!("OUT_DIR"), "/rust_snippets.rs"));

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

#[test]
fn every_rust_block_compiles_and_runs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let json = std::fs::read_to_string(root.join("docs/examples/hello.json")).unwrap();
    let dir = std::env::temp_dir().join(format!("gridwell-rust-snippets-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("table.json"), &json).unwrap();
    std::env::set_current_dir(&dir).unwrap();

    let mut failures = Vec::new();
    for (location, snippet) in SNIPPETS {
        let table = gridwell::Table::from_json(&json).unwrap();
        match catch_unwind(AssertUnwindSafe(|| snippet(json.clone(), table))) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => failures.push(format!("{location}: returned an error: {e}")),
            Err(_) => failures.push(format!("{location}: panicked (see above)")),
        }
    }
    std::env::set_current_dir(root).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // The pages hold this many; fewer means the extraction lost some.
    assert!(SNIPPETS.len() >= 13, "only {} Rust blocks", SNIPPETS.len());

    extraction_plants();
}

/// Planted pages: the extraction keeps exactly the Rust blocks.
fn extraction_plants() {
    use snippets::{snippets, Snippet, SKIP};
    let page = format!(
        "# T\n\n```rust\nlet a = 1;\n```\n\n```{{.rust filename=\"x.rs\"}}\nlet b = 2;\n```\n\n\
         ```toml\n[x]\n```\n\n```rusty\nno\n```\n\n{SKIP}\n\n```rust\nskipped();\n```\n\n\
         ````markdown\n```rust\ninside();\n```\n````\n\n```rust\nlast();\n```\n"
    );
    let found = snippets("p.qmd", &page);
    let at = |line, code: &str| Snippet {
        page: "p.qmd".to_string(),
        line,
        code: code.to_string(),
    };
    assert_eq!(
        found,
        [
            at(3, "let a = 1;\n"),
            at(7, "let b = 2;\n"),
            at(31, "last();\n")
        ],
        "{found:#?}"
    );
    // The marker applies to the next fence only.
    let page = format!("{SKIP}\n\nText.\n\n```rust\nkept();\n```\n");
    assert_eq!(snippets("p.qmd", &page).len(), 1);
}
