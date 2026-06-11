#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz the validator: parse JSON, then validate.
// Both parse and validate must never panic regardless of input.
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(table) = gridwell_ir::Table::from_json(s) {
            let _ = table.validate();
        }
    }
});
