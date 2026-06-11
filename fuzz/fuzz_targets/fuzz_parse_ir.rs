#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz the IR JSON parser with arbitrary byte input.
// Must never panic — only return Ok/Err.
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = gridwell_ir::Table::from_json(s);
    }
});
