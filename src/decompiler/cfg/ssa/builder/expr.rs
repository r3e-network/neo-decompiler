//! Opcode-to-expression lowering for the SSA builder.

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::helpers::value_type_from_operand;
use crate::decompiler::ir::{Intrinsic, SemanticCallTarget};
use crate::instruction::{Instruction, OpCode};

use super::helpers::{binary_op_for, literal_for_push, unary_op_for};
use super::{unknown_var, SsaBuilder, SsaExpr, SsaVariable};

impl<'a> SsaBuilder<'a> {
    /// Build the [`SsaExpr`] for a compute opcode given its already-popped
    /// operands (`popped`, ordered deep-to-top). Compute opcodes get a real
    /// binary/unary/literal tree; everything else surfaces as a `Call` placeholder
    /// that still preserves data-flow (the popped vars appear as arguments).
    pub(super) fn build_expr(
        &self,
        op: OpCode,
        instr: &Instruction,
        popped: &[SsaVariable],
    ) -> SsaExpr {
        if let Some(lit) = literal_for_push(op, instr) {
            return SsaExpr::lit(lit);
        }
        if let Some(bin) = binary_op_for(op) {
            let mut it = popped.iter();
            let left = it.next().cloned().unwrap_or_else(unknown_var);
            let right = it.next().cloned().unwrap_or_else(unknown_var);
            return SsaExpr::binary(bin, SsaExpr::var(left), SsaExpr::var(right));
        }
        if op == OpCode::Convert {
            let value = popped.first().cloned().unwrap_or_else(unknown_var);
            let target = instr
                .operand
                .as_ref()
                .and_then(value_type_from_operand)
                .unwrap_or(ValueType::Unknown);
            return SsaExpr::Convert {
                value: Box::new(SsaExpr::var(value)),
                target,
            };
        }
        if op == OpCode::Istype {
            let value = popped.first().cloned().unwrap_or_else(unknown_var);
            let target = instr
                .operand
                .as_ref()
                .and_then(value_type_from_operand)
                .unwrap_or(ValueType::Unknown);
            return SsaExpr::IsType {
                value: Box::new(SsaExpr::var(value)),
                target,
            };
        }
        if matches!(op, OpCode::Newarray | OpCode::NewarrayT) {
            let length = popped.first().cloned().unwrap_or_else(unknown_var);
            let element_type = (op == OpCode::NewarrayT)
                .then(|| instr.operand.as_ref().and_then(value_type_from_operand))
                .flatten();
            return SsaExpr::NewArray {
                length: Box::new(SsaExpr::var(length)),
                element_type,
            };
        }
        if matches!(
            op,
            OpCode::Within | OpCode::Substr | OpCode::Modmul | OpCode::Modpow
        ) {
            return intrinsic_call(op, popped);
        }
        if let Some(un) = unary_op_for(op) {
            let operand = popped.first().cloned().unwrap_or_else(unknown_var);
            return SsaExpr::unary(un, SsaExpr::var(operand));
        }
        if matches!(
            op,
            OpCode::Sqrt
                | OpCode::Nz
                | OpCode::Size
                | OpCode::Keys
                | OpCode::Values
                | OpCode::Isnull
        ) {
            return intrinsic_call(op, popped);
        }
        intrinsic_call(op, popped)
    }
}

fn intrinsic_call(op: OpCode, popped: &[SsaVariable]) -> SsaExpr {
    SsaExpr::call(
        SemanticCallTarget::Intrinsic(Intrinsic::Opcode(op)),
        popped.iter().cloned().map(SsaExpr::var).collect(),
    )
}
