#![no_main]

use gridwell_core::{Color, Length};
use libfuzzer_sys::fuzz_target;

// Fuzz the colour and length parsers: never panic, and whatever parses must
// survive a round trip through the normalized form writers emit.
fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(c) = s.parse::<Color>() {
        assert_eq!(c.to_hex().parse::<Color>().unwrap(), c, "{s:?}");
        assert_eq!(c.to_css().parse::<Color>().unwrap(), c, "{s:?}");
    }
    if let Ok(l) = s.parse::<Length>() {
        assert!(l.value().is_none_or(f64::is_finite), "{s:?}");
        assert_eq!(l.to_string().parse::<Length>().unwrap(), l, "{s:?}");
    }
});
