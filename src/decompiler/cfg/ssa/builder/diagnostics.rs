//! Instruction fidelity and stack-loss diagnostics for SSA lowering.

use crate::decompiler::cfg::method_body::{
    classify_instruction, Fidelity, LoweringIssue, LoweringIssueKind, OpcodeFidelity,
};
use crate::instruction::{Instruction, OpCode, Operand, OperandEncoding};

pub(super) fn record_instruction_ceiling(
    instruction: &Instruction,
    issues: &mut Vec<LoweringIssue>,
) {
    match classify_instruction(instruction) {
        OpcodeFidelity::Exact => {}
        OpcodeFidelity::Conservative => {
            let detail = if matches!(instruction.opcode, OpCode::Abort | OpCode::Abortmsg) {
                format!(
                    "{} is represented as a catchable exception because an uncatchable VM abort has no direct structured equivalent",
                    instruction.opcode.mnemonic()
                )
            } else {
                format!(
                    "{} is preserved as a low-level call",
                    instruction.opcode.mnemonic()
                )
            };
            issues.push(LoweringIssue {
                offset: instruction.offset,
                opcode: instruction.opcode,
                kind: LoweringIssueKind::MissingProvenance,
                fidelity: Fidelity::Conservative,
                detail,
            });
        }
        OpcodeFidelity::Incomplete(kind) => record_incomplete_issue(
            instruction,
            kind,
            format!(
                "{} semantics are not represented exactly",
                instruction.opcode.mnemonic()
            ),
            issues,
        ),
    }
}

pub(super) fn record_missing_operand_metadata(
    instruction: &Instruction,
    issues: &mut Vec<LoweringIssue>,
) {
    let encoding = instruction.opcode.operand_encoding();
    if operand_matches_encoding(encoding, instruction.operand.as_ref()) {
        return;
    }
    record_incomplete_issue(
        instruction,
        LoweringIssueKind::MissingOperandMetadata,
        format!(
            "{} requires {encoding:?} operand metadata",
            instruction.opcode.mnemonic()
        ),
        issues,
    );
}

fn operand_matches_encoding(encoding: OperandEncoding, operand: Option<&Operand>) -> bool {
    match (encoding, operand) {
        (OperandEncoding::None, _) => true,
        (OperandEncoding::I8, Some(Operand::I8(_)))
        | (OperandEncoding::I16, Some(Operand::I16(_)))
        | (OperandEncoding::I32, Some(Operand::I32(_)))
        | (OperandEncoding::I64, Some(Operand::I64(_)))
        | (OperandEncoding::Jump8, Some(Operand::Jump(_)))
        | (OperandEncoding::Jump32, Some(Operand::Jump32(_)))
        | (OperandEncoding::U8, Some(Operand::U8(_)))
        | (OperandEncoding::U16, Some(Operand::U16(_)))
        | (OperandEncoding::U32, Some(Operand::U32(_)))
        | (OperandEncoding::Syscall, Some(Operand::Syscall(_)))
        | (
            OperandEncoding::Data1 | OperandEncoding::Data2 | OperandEncoding::Data4,
            Some(Operand::Bytes(_)),
        ) => true,
        (OperandEncoding::Bytes(expected), Some(Operand::Bytes(bytes))) => bytes.len() == expected,
        _ => false,
    }
}

pub(super) fn record_incomplete_issue(
    instruction: &Instruction,
    kind: LoweringIssueKind,
    detail: impl Into<String>,
    issues: &mut Vec<LoweringIssue>,
) {
    issues.push(LoweringIssue {
        offset: instruction.offset,
        opcode: instruction.opcode,
        kind,
        fidelity: Fidelity::Incomplete,
        detail: detail.into(),
    });
}

pub(super) fn record_stack_underflow(
    instruction: &Instruction,
    required: usize,
    available: usize,
    issues: &mut Vec<LoweringIssue>,
) {
    record_incomplete_issue(
        instruction,
        LoweringIssueKind::LostStackValue,
        format!("requires {required} stack values, but only {available} are available"),
        issues,
    );
}

pub(super) fn fixed_reorder_arity(opcode: OpCode) -> Option<usize> {
    match opcode {
        OpCode::Depth => None,
        OpCode::Drop | OpCode::Dup => Some(1),
        OpCode::Nip | OpCode::Over | OpCode::Tuck | OpCode::Swap => Some(2),
        OpCode::Rot | OpCode::Reverse3 => Some(3),
        OpCode::Reverse4 => Some(4),
        _ => None,
    }
}
