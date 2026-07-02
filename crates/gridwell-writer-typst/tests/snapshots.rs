//! Textual snapshot regression tests over the shared example corpus.
//!
//! One named snapshot per example (`snapshots/snapshots__<name>.snap`).
//! Review changes with `cargo insta review`; regenerate with
//! `INSTA_UPDATE=always cargo test`.

use gridwell_testkit::examples;
use gridwell_writer_typst::render_typst;

#[test]
fn snapshots() {
    for ex in examples() {
        let out = render_typst(&ex.table())
            .unwrap_or_else(|e| panic!("render '{}' failed: {e}", ex.name));
        insta::assert_snapshot!(ex.name, out);
    }
}
