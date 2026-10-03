#![no_main]

use gridwell_layout::{resolve, Slot};
use libfuzzer_sys::fuzz_target;

// Resolve any parseable IR, valid or not: never panic, and keep the grid's
// structural invariants (one slot per visible column; every covered slot points
// at an origin whose rectangle contains it; origins stay inside their section).
fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(table) = gridwell_ir::Table::from_json(s) else {
        return;
    };
    // Bound the work like the validator does before allocating grids.
    if table.config.table_cols > 64 || s.len() > 65_536 {
        return;
    }
    let rt = resolve(&table);
    let width = rt.columns.len();
    for section in rt.sections() {
        for (r, row) in section.rows.iter().enumerate() {
            assert_eq!(row.slots.len(), width);
            for (c, slot) in row.slots.iter().enumerate() {
                match slot {
                    Slot::Origin(cell) => {
                        assert_eq!(cell.col, c);
                        assert!(c + cell.colspan <= width);
                        assert!(r + cell.rowspan <= section.rows.len());
                    }
                    Slot::CoveredH { origin_col } => {
                        let o = row.slots[*origin_col].origin().expect("CoveredH origin");
                        assert!(*origin_col < c && c < origin_col + o.colspan);
                    }
                    Slot::CoveredV { origin_row, origin_col, .. } => {
                        let o = section.rows[*origin_row].slots[*origin_col]
                            .origin()
                            .expect("CoveredV origin");
                        assert!(*origin_row < r && r < origin_row + o.rowspan);
                        assert!(*origin_col <= c && c < origin_col + o.colspan);
                    }
                    Slot::Empty => {}
                }
            }
        }
        for cells in section.continuation_rows() {
            assert_eq!(cells.iter().map(|m| m.span).sum::<usize>(), width);
        }
    }
});
