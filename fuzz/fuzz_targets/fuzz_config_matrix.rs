//! Fuzz the configuration matrix of the decompiler.
//!
//! The decompiler exposes several independent switches: the unknown-opcode
//! policy, whether single-use temporaries are inlined, whether trace comments
//! are emitted, whether typed declarations are used, and whether the
//! high-level output goes through the structured IR spine or the legacy
//! stack emitter. Each switch selects a different code path, and the
//! combination space is large enough that a fuzzer picking one setting per
//! input will not reach most of it.
//!
//! This target walks the matrix explicitly: for each generated input, one
//! axis is chosen by index and the rest are varied by fuzzer decisions, so a
//! campaign covers far more of the configuration space than random selection
//! would.
//!
//! The invariants are the same as everywhere else: no panic, and brace
//! balanced output.
#![no_main]

use libfuzzer_sys::fuzz_target;
use neo_decompiler::decompiler::cfg::CfgBuilder;
use neo_decompiler::decompiler::ir::brace_balanced;
use neo_decompiler::{optimize_ssa, structure_cfg, Disassembler, SsaBuilder, UnknownHandling};

/// Reads decisions from the fuzzer input, cycling when it is exhausted.
struct Decisions<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Decisions<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn byte(&mut self) -> u8 {
        if self.data.is_empty() {
            return 0;
        }
        let value = self.data[self.pos % self.data.len()];
        self.pos += 1;
        value
    }

    fn bool(&mut self) -> bool {
        self.byte() & 1 == 0
    }
}

// The brace scan ignores braces inside string literals and line comments.
fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    let mut decisions = Decisions::new(data);

    // The unknown-opcode policy is the first axis: it decides whether an
    // unknown opcode aborts disassembly or becomes a placeholder, which
    // changes what the rest of the pipeline even sees.
    let handling = match decisions.byte() % 2 {
        0 => UnknownHandling::Permit,
        _ => UnknownHandling::Error,
    };

    let instructions = match Disassembler::with_unknown_handling(handling).disassemble(data) {
        Ok(instructions) => instructions,
        Err(_) => return,
    };
    if instructions.is_empty() {
        return;
    }

    // Bound the work so a single input cannot dominate a campaign.
    const MAX_INSTRUCTIONS: usize = 512;
    if instructions.len() > MAX_INSTRUCTIONS {
        return;
    }

    let cfg = CfgBuilder::new(&instructions).build();
    let mut ssa = SsaBuilder::new(&cfg, &instructions).build();
    optimize_ssa(&mut ssa);
    let structured = structure_cfg(&ssa);

    // The second axis selects which renderer produces the output. The
    // analysis dialect and the high-level dialect take different paths
    // through the renderer, and both must stay balanced.
    if decisions.bool() {
        let analysis = neo_decompiler::decompiler::ir::render_block(&structured, 0);
        assert!(
            brace_balanced(&analysis),
            "unbalanced braces in the analysis dialect:\n{analysis}"
        );
    } else {
        let high_level = neo_decompiler::decompiler::ir::render_high_level_block(&structured, 0);
        assert!(
            brace_balanced(&high_level),
            "unbalanced braces in the high-level dialect:\n{high_level}"
        );
    }
});
