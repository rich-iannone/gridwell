#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz the HTML writer: parse JSON, then render to HTML.
// Must never panic — only return Ok/Err.
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(table) = gridwell_ir::Table::from_json(s) {
            let _ = gridwell_writer_html::render_html(&table);
        }
    }
});
