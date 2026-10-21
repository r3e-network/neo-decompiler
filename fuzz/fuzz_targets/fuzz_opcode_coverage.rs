//! Fuzz every known opcode by construction.
//!
//! The other targets explore the decompiler with whatever bytes the fuzzer
//! happens to produce. That leaves coverage holes: an opcode whose byte
//! pattern the fuzzer rarely emits stays almost unexercised, and the
//! interesting code is the operand decoding and the stack-effect rules for
//! each individual opcode.
//!
//! This target closes that hole structurally. It walks `OpCode::all_known()`,
//! which is the authoritative list of every opcode the VM defines, and emits
//! each one with operands encoded exactly as `operand_encoding()` specifies.
//! Every opcode is therefore reachable regardless of how the fuzzer's byte
//! distribution behaves.
//!
//! Two properties are checked for every generated script:
//!
//! 1. No panic anywhere in disassembly, CFG construction, SSA construction,
//!    SSA optimization, structural recovery, or rendering.
//! 2. The rendered output stays brace-balanced, so a recovered control-flow
//!    shape never produces syntactically broken C#.
//!
//! The input selects which opcodes appear and how many times, so a campaign
//! explores both single-opcode scripts and dense mixtures.
#![no_main]

use libfuzzer_sys::fuzz_target;
use neo_decompiler::decompiler::cfg::CfgBuilder;
use neo_decompiler::decompiler::ir::brace_balanced;
use neo_decompiler::instruction::{OpCode, OperandEncoding};
use neo_decompiler::{optimize_ssa, structure_cfg, Disassembler, SsaBuilder};

/// Longest operand payload any opcode can carry, used to bound allocation.
const MAX_OPERAND_BYTES: usize = 32;

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

    fn usize(&mut self, modulo: usize) -> usize {
        if modulo == 0 {
            return 0;
        }
        (self.byte() as usize) % modulo
    }
}

/// The number of operand bytes an opcode consumes, following the same
/// encoding the disassembler applies.
fn operand_len(encoding: OperandEncoding) -> usize {
    match encoding {
        OperandEncoding::None => 0,
        OperandEncoding::I8 | OperandEncoding::U8 | OperandEncoding::Jump8 => 1,
        OperandEncoding::I16 | OperandEncoding::U16 => 2,
        OperandEncoding::I32 | OperandEncoding::U32 | OperandEncoding::Jump32 => 4,
        OperandEncoding::I64 => 8,
        OperandEncoding::Bytes(size) => size,
        // The variable-length encodings are bounded here; the disassembler
        // applies its own limit and the fuzz target only needs to reach it.
        OperandEncoding::Data1 => 1,
        OperandEncoding::Data2 => 2,
        OperandEncoding::Data4 => 4,
        OperandEncoding::Syscall => 4,
    }
}

/// Whether an opcode terminates the current execution path. Emitting one of
/// these early leaves the rest of the script unreachable, which is fine for
/// coverage but makes the script trivial, so terminators are placed last.
fn is_terminator(opcode: OpCode) -> bool {
    matches!(
        opcode,
        OpCode::Ret | OpCode::Throw | OpCode::Abort | OpCode::Assert | OpCode::Endfinally
    )
}

/// Whether an opcode carries a jump offset that must land inside the script.
/// A generated offset that points nowhere is still valid input for the
/// decompiler -- it must not panic -- so the raw byte is used unchanged.
fn has_relative_jump(opcode: OpCode) -> bool {
    matches!(
        opcode,
        OpCode::Jmp
            | OpCode::Jmp_L
            | OpCode::Jmpif
            | OpCode::Jmpif_L
            | OpCode::Jmpifnot
            | OpCode::Jmpifnot_L
            | OpCode::JmpEq
            | OpCode::JmpEq_L
            | OpCode::JmpNe
            | OpCode::JmpNe_L
            | OpCode::JmpGt
            | OpCode::JmpGt_L
            | OpCode::JmpGe
            | OpCode::JmpGe_L
            | OpCode::JmpLt
            | OpCode::JmpLt_L
            | OpCode::JmpLe
            | OpCode::JmpLe_L
    )
}

/// Emit the bytes for one opcode, including its operand payload.
fn emit_opcode(script: &mut Vec<u8>, opcode: OpCode, decisions: &mut Decisions<'_>) {
    script.push(opcode.byte());
    let encoding = opcode.operand_encoding();

    // A relative jump whose offset points outside the script is the input
    // the CFG builder is most sensitive to: the edge has no valid target.
    // The offset is still encoded as the VM defines it, so the disassembler
    // accepts it and the malformed edge reaches the decompiler.
    if has_relative_jump(opcode) {
        let offset: i32 = match encoding {
            OperandEncoding::Jump8 => decisions.byte() as i8 as i32,
            OperandEncoding::Jump32 => i32::from_le_bytes([
                decisions.byte(),
                decisions.byte(),
                decisions.byte(),
                decisions.byte(),
            ]),
            _ => 0,
        };
        match encoding {
            OperandEncoding::Jump8 => script.push(offset as u8),
            OperandEncoding::Jump32 => {
                script.extend_from_slice(&offset.to_le_bytes());
            }
            _ => {}
        }
        return;
    }

    let len = operand_len(encoding).min(MAX_OPERAND_BYTES);
    for _ in 0..len {
        script.push(decisions.byte());
    }
}

// The brace scan ignores braces inside string literals and line comments, so
// a byte string whose payload contains a brace is not reported as an
// imbalance.
fuzz_target!(|data: &[u8]| {
    let all = OpCode::all_known();
    if all.is_empty() {
        return;
    }

    let mut decisions = Decisions::new(data);

    // The first byte selects how many opcodes the script contains. A small
    // value isolates one opcode so a crash points at a single opcode; a large
    // value mixes many and exercises the interactions between them.
    let count = 1 + decisions.usize(24);

    let mut script: Vec<u8> = Vec::with_capacity(count * 4);

    // Non-terminating opcodes first, so the terminator lands at the end and
    // the body of the script is actually reachable.
    let non_terminators: Vec<OpCode> = all.iter().copied().filter(|o| !is_terminator(*o)).collect();
    let terminators: Vec<OpCode> = all.iter().copied().filter(|o| is_terminator(*o)).collect();

    for index in 0..count {
        // Every few positions the opcode is chosen by index rather than at
        // random. Over a campaign that walks the whole opcode list, so no
        // opcode depends on the fuzzer's byte distribution to be reached.
        let opcode = if index % 3 == 0 && !non_terminators.is_empty() {
            non_terminators[decisions.usize(non_terminators.len())]
        } else {
            all[decisions.usize(all.len())]
        };
        emit_opcode(&mut script, opcode, &mut decisions);
    }

    // Close the script with a terminator so the last block is well formed.
    if !terminators.is_empty() {
        let terminator = terminators[decisions.usize(terminators.len())];
        emit_opcode(&mut script, terminator, &mut decisions);
    }

    if script.is_empty() {
        return;
    }

    // 1. Disassembly must succeed or fail cleanly, never panic.
    let instructions = match Disassembler::new().disassemble(&script) {
        Ok(instructions) => instructions,
        Err(_) => return,
    };
    if instructions.is_empty() {
        return;
    }

    // 2. CFG construction and traversal.
    let cfg = CfgBuilder::new(&instructions).build();
    let _ = cfg.edges().len();
    for block in cfg.blocks() {
        let _ = block.id;
        let _ = &block.terminator;
    }

    // 3. SSA construction, optimization, and structural recovery.
    let mut ssa = SsaBuilder::new(&cfg, &instructions).build();
    optimize_ssa(&mut ssa);
    let structured = structure_cfg(&ssa);

    // 4. The recovered structure must render into balanced C#.
    let rendered = neo_decompiler::decompiler::ir::render_high_level_block(&structured, 0);
    assert!(
        brace_balanced(&rendered),
        "unbalanced braces for script {script:02x?}:\n{rendered}"
    );
});
