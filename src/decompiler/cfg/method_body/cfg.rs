use std::collections::BTreeSet;

use crate::decompiler::cfg::{BasicBlock, Cfg, CfgBuilder, Terminator};
use crate::instruction::{Instruction, OpCode, Operand};

/// Build a self-contained CFG for one method slice.
pub(crate) fn build_method_cfg(instructions: &[Instruction], start: usize, end: usize) -> Cfg {
    build_method_cfg_with_non_returning_calls(instructions, start, end, &BTreeSet::new())
}

/// Build a method CFG while treating resolved non-returning calls as terminal.
pub(crate) fn build_method_cfg_with_non_returning_calls(
    instructions: &[Instruction],
    start: usize,
    end: usize,
    non_returning_calls: &BTreeSet<usize>,
) -> Cfg {
    let built = CfgBuilder::new(instructions)
        .with_non_returning_calls(non_returning_calls.iter().copied())
        .build();
    let mut cfg = Cfg::new();

    for block in built.blocks() {
        let mut block = block.clone();
        if control_transfer_leaves_method(&block, instructions, start, end) {
            block.terminator = Terminator::Return;
        }
        cfg.add_block(block);
    }

    for edge in built.edges() {
        let retained = cfg
            .block(edge.from)
            .is_some_and(|block| block.terminator.successors().contains(&edge.to));
        if retained {
            cfg.add_edge(edge.from, edge.to, edge.kind);
        }
    }
    cfg
}

fn control_transfer_leaves_method(
    block: &BasicBlock,
    instructions: &[Instruction],
    start: usize,
    end: usize,
) -> bool {
    let Some(last_index) = block.instruction_range.end.checked_sub(1) else {
        return false;
    };
    let Some(instruction) = instructions.get(last_index) else {
        return false;
    };
    let is_conditional = matches!(
        instruction.opcode,
        OpCode::Jmpif
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
    );
    let is_jump = is_conditional || matches!(instruction.opcode, OpCode::Jmp | OpCode::Jmp_L);
    if !is_jump {
        return false;
    }

    let target = match instruction.operand {
        Some(Operand::Jump(delta)) => instruction.offset.checked_add_signed(delta as isize),
        Some(Operand::Jump32(delta)) => instruction.offset.checked_add_signed(delta as isize),
        _ => None,
    };
    let target_leaves = target.is_some_and(|target| target < start || target >= end);
    let fallthrough_leaves = is_conditional && instructions.get(last_index + 1).is_none();
    target_leaves || fallthrough_leaves
}
