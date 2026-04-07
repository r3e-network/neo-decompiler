#![no_main]

use libfuzzer_sys::fuzz_target;
use neo_decompiler::{CfgBuilder, Disassembler, SsaConversion};

fuzz_target!(|data: &[u8]| {
    // Skip empty inputs -- nothing useful to exercise.
    if data.is_empty() {
        return;
    }

    // 1. Disassemble raw bytecode, bypassing NEF header validation entirely.
    let disassembler = Disassembler::new();
    let instructions = match disassembler.disassemble(data) {
        Ok(instrs) => instrs,
        Err(_) => return,
    };

    if instructions.is_empty() {
        return;
    }

    // 2. Build the control-flow graph from decoded instructions.
    let cfg = CfgBuilder::new(&instructions).build();

    // 3. Exercise CFG traversal and queries.
    for block in cfg.blocks() {
        let _ = block.id;
        let _ = &block.instruction_range;
        let _ = &block.terminator;
    }
    let _ = cfg.edges().len();

    // 4. Attempt SSA conversion (dominance computation + phi insertion).
    let _ = cfg.to_ssa();
});
