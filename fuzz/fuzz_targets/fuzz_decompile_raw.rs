#![no_main]

use libfuzzer_sys::fuzz_target;
use neo_decompiler::{optimize_ssa, structure_cfg, CfgBuilder, Disassembler, SsaBuilder};

fuzz_target!(|data: &[u8]| {
    // Skip empty inputs -- nothing useful to exercise.
    if data.is_empty() {
        return;
    }

    // 1. Disassemble raw bytecode, bypassing NEF header validation entirely.
    // Default disassembler permits unknown opcodes so malformed scripts still
    // reach CFG/SSA construction (the panic surface we care about).
    let instructions = match Disassembler::new().disassemble(data) {
        Ok(instrs) => instrs,
        Err(_) => return,
    };

    if instructions.is_empty() {
        return;
    }

    // Bound work per input so ASAN fuzz campaigns stay responsive. Production
    // decompile of real NEFs is tiny; this only skips pathological raw scripts.
    const MAX_INSTRUCTIONS: usize = 512;
    if instructions.len() > MAX_INSTRUCTIONS {
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

    // 4. Stack-effect SSA construction + optimization + structural recovery.
    // These must never panic on malformed bytecode graphs.
    let mut ssa = SsaBuilder::new(&cfg, &instructions).build();
    optimize_ssa(&mut ssa);
    let _ = structure_cfg(&ssa);
    let _ = ssa.block_count();
});
