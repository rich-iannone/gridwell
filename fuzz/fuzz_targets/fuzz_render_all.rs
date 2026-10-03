#![no_main]

use libfuzzer_sys::fuzz_target;

// Any IR that validates must render in every registered format: no panic, no
// error. (Invalid IR is refused by the registry before any writer runs.)
fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(table) = gridwell_ir::Table::from_json(s) else {
        return;
    };
    // Keep each input cheap: the corpus tables are small.
    if table.config.table_cols > 64 || s.len() > 65_536 {
        return;
    }
    if table.validate().is_empty() {
        for name in gridwell_render::names() {
            if let Err(e) = gridwell_render::render(&table, name, None) {
                panic!("{name} failed on valid IR: {e}");
            }
        }
    }
});
