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

    // The graph has to be consistent with itself: every block it names as
    // a successor has to be a block it contains. A dangling edge means
    // the builder produced a graph the rest of the pipeline cannot walk,
    // and the failure shows up far away from its cause.
    let block_count = cfg.block_count();
    assert!(
        block_count > 0,
        "a script that disassembled into {} instructions produced no blocks",
        instructions.len()
    );
    for block in cfg.blocks() {
        for successor in cfg.successors(block.id) {
            assert!(
                successor.index() < block_count,
                "the graph points at block {}, but it only has {block_count} blocks",
                successor.index()
            );
        }
    }

    // 4. Stack-effect SSA construction + optimization + structural recovery.
    // These must never panic on malformed bytecode graphs.
    let mut ssa = SsaBuilder::new(&cfg, &instructions).build();
    optimize_ssa(&mut ssa);
    let _ = ssa.block_count();

    // The structural recovery has to terminate and has to produce a
    // bounded result. A recovery that revisits a cycle grows the emitted
    // block without bound, which shows up as an out-of-memory kill rather
    // than a crash the campaign can report.
    //
    // The bound is generous: a well-formed script produces a handful of
    // statements, and a malformed one may produce more, but the recovery
    // must not emit a number of statements that grows with the number of
    // times it walked a cycle.
    let structured = structure_cfg(&ssa);
    let statement_count = structured.stmts.len();
    assert!(
        statement_count <= instructions.len() * 4 + 64,
        "the structural recovery emitted {statement_count} statements for {} instructions",
        instructions.len()
    );
});
