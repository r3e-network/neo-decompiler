#![no_main]

use libfuzzer_sys::fuzz_target;
use neo_decompiler::NefParser;

fuzz_target!(|data: &[u8]| {
    let parser = NefParser::new();
    // We don't care about the result, only that it doesn't panic
    let _ = parser.parse(data);
});
