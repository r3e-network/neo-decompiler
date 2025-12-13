#![no_main]

use libfuzzer_sys::fuzz_target;
use neo_decompiler::Disassembler;

fuzz_target!(|data: &[u8]| {
    let disassembler = Disassembler::new();
    // We don't care about the result, only that it doesn't panic
    let _ = disassembler.disassemble(data);
});
