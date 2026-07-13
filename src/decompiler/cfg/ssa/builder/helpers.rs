// Opcode and expression helpers for stack-effect SSA construction.

use super::*;

pub(super) fn literal_for_push(op: OpCode, instr: &Instruction) -> Option<Literal> {
    use OpCode::*;
    match op {
        Push0 => Some(Literal::Int(0)),
        Push1 => Some(Literal::Int(1)),
        Push2 => Some(Literal::Int(2)),
        Push3 => Some(Literal::Int(3)),
        Push4 => Some(Literal::Int(4)),
        Push5 => Some(Literal::Int(5)),
        Push6 => Some(Literal::Int(6)),
        Push7 => Some(Literal::Int(7)),
        Push8 => Some(Literal::Int(8)),
        Push9 => Some(Literal::Int(9)),
        Push10 => Some(Literal::Int(10)),
        Push11 => Some(Literal::Int(11)),
        Push12 => Some(Literal::Int(12)),
        Push13 => Some(Literal::Int(13)),
        Push14 => Some(Literal::Int(14)),
        Push15 => Some(Literal::Int(15)),
        Push16 => Some(Literal::Int(16)),
        PushM1 => Some(Literal::Int(-1)),
        PushT => Some(Literal::Bool(true)),
        PushF => Some(Literal::Bool(false)),
        PushNull => Some(Literal::Null),
        PushA => match &instr.operand {
            Some(Operand::I32(delta)) => instr
                .offset
                .checked_add_signed(*delta as isize)
                .and_then(|target| i64::try_from(target).ok())
                .map(Literal::Int),
            _ => None,
        },
        Pushint8 | Pushint16 | Pushint32 | Pushint64 => match &instr.operand {
            Some(Operand::I8(v)) => Some(Literal::Int(i64::from(*v))),
            Some(Operand::I16(v)) => Some(Literal::Int(i64::from(*v))),
            Some(Operand::I32(v)) => Some(Literal::Int(i64::from(*v))),
            Some(Operand::I64(v)) => Some(Literal::Int(*v)),
            _ => None,
        },
        Pushint128 | Pushint256 => match &instr.operand {
            Some(Operand::Bytes(bytes)) => Some(Literal::BigInt(signed_le_bytes_to_decimal(bytes))),
            _ => None,
        },
        Pushdata1 | Pushdata2 | Pushdata4 => match &instr.operand {
            Some(Operand::Bytes(bytes)) => Some(
                printable_utf8(bytes)
                    .map(Literal::String)
                    .unwrap_or_else(|| Literal::Bytes(bytes.clone())),
            ),
            _ => None,
        },
        _ => None,
    }
}

/// Map a binary compute opcode to its IR operator, if applicable.
pub(super) fn binary_op_for(op: OpCode) -> Option<BinOp> {
    use OpCode::*;
    Some(match op {
        Add => BinOp::Add,
        Sub => BinOp::Sub,
        Mul => BinOp::Mul,
        Div => BinOp::Div,
        Mod => BinOp::Mod,
        Pow => BinOp::Pow,
        Shl => BinOp::Shl,
        Shr => BinOp::Shr,
        And => BinOp::And,
        Or => BinOp::Or,
        Xor => BinOp::Xor,
        Equal | Numequal => BinOp::Eq,
        Notequal | Numnotequal => BinOp::Ne,
        Lt => BinOp::Lt,
        Le => BinOp::Le,
        Gt => BinOp::Gt,
        Ge => BinOp::Ge,
        Booland => BinOp::LogicalAnd,
        Boolor => BinOp::LogicalOr,
        _ => return None,
    })
}

pub(super) fn is_boolean_branch(op: OpCode) -> bool {
    matches!(
        op,
        OpCode::Jmpif | OpCode::Jmpif_L | OpCode::Jmpifnot | OpCode::Jmpifnot_L
    )
}

pub(super) fn comparison_branch_op(op: OpCode) -> Option<BinOp> {
    use OpCode::*;
    Some(match op {
        JmpEq | JmpEq_L => BinOp::Eq,
        JmpNe | JmpNe_L => BinOp::Ne,
        JmpGt | JmpGt_L => BinOp::Gt,
        JmpGe | JmpGe_L => BinOp::Ge,
        JmpLt | JmpLt_L => BinOp::Lt,
        JmpLe | JmpLe_L => BinOp::Le,
        _ => return None,
    })
}

pub(super) fn is_effectful_collection(op: OpCode) -> bool {
    matches!(
        op,
        OpCode::Setitem
            | OpCode::Append
            | OpCode::Remove
            | OpCode::Clearitems
            | OpCode::Reverseitems
            | OpCode::Memcpy
    )
}

pub(super) fn is_collection_mutation(op: OpCode) -> bool {
    is_effectful_collection(op) || op == OpCode::Popitem
}

pub(super) fn is_shape_preserving_collection_mutation(op: OpCode) -> bool {
    matches!(op, OpCode::Setitem | OpCode::Reverseitems | OpCode::Memcpy)
}

/// Map a unary compute opcode to its IR operator, if applicable.
pub(super) fn unary_op_for(op: OpCode) -> Option<UnaryOp> {
    use OpCode::*;
    Some(match op {
        Inc => UnaryOp::Inc,
        Dec => UnaryOp::Dec,
        Negate => UnaryOp::Neg,
        Abs => UnaryOp::Abs,
        Sign => UnaryOp::Sign,
        Not => UnaryOp::LogicalNot,
        Invert => UnaryOp::Not,
        _ => return None,
    })
}

/// A short mnemonic for call-placeholder expressions.
pub(super) fn mnemonic(op: OpCode) -> String {
    format!("{op:?}").to_lowercase()
}

pub(super) fn call_name(op: OpCode, instruction: &Instruction) -> String {
    match (op, &instruction.operand) {
        (OpCode::Call | OpCode::Call_L, Some(Operand::Jump(delta))) => instruction
            .offset
            .checked_add_signed(*delta as isize)
            .map_or_else(
                || "call".to_string(),
                |target| format!("call_0x{target:04X}"),
            ),
        (OpCode::Call | OpCode::Call_L, Some(Operand::Jump32(delta))) => instruction
            .offset
            .checked_add_signed(*delta as isize)
            .map_or_else(
                || "call".to_string(),
                |target| format!("call_0x{target:04X}"),
            ),
        (OpCode::CallT, Some(Operand::U16(index))) => format!("callt_0x{index:04X}"),
        (OpCode::CallA, _) => "calla".to_string(),
        _ => mnemonic(op),
    }
}

pub(super) fn context_free_call_target(instruction: &Instruction) -> SemanticCallTarget {
    let internal = |offset: Option<usize>| {
        offset.map_or_else(
            || SemanticCallTarget::Unresolved {
                display_name: call_name(instruction.opcode, instruction),
            },
            |offset| SemanticCallTarget::Internal {
                offset,
                name: format!("call_0x{offset:04X}"),
            },
        )
    };

    match (instruction.opcode, &instruction.operand) {
        (OpCode::Call, Some(Operand::Jump(delta))) => {
            internal(instruction.offset.checked_add_signed(*delta as isize))
        }
        (OpCode::Call_L, Some(Operand::Jump32(delta))) => {
            internal(instruction.offset.checked_add_signed(*delta as isize))
        }
        (OpCode::CallT, Some(Operand::U16(index))) => SemanticCallTarget::MethodToken {
            index: usize::from(*index),
            name: format!("callt_0x{index:04X}"),
            hash_le: None,
            call_flags: None,
        },
        _ => SemanticCallTarget::Unresolved {
            display_name: call_name(instruction.opcode, instruction),
        },
    }
}

/// Collect every [`SsaVariable`] referenced by an [`SsaExpr`].
pub(super) fn collect_expr_uses(expr: &SsaExpr) -> Vec<SsaVariable> {
    let mut out = Vec::new();
    collect_expr_uses_into(expr, &mut out);
    out
}

pub(super) fn collect_expr_uses_into(expr: &SsaExpr, out: &mut Vec<SsaVariable>) {
    match expr {
        SsaExpr::Variable(v) => out.push(v.clone()),
        SsaExpr::Binary { left, right, .. } => {
            collect_expr_uses_into(left, out);
            collect_expr_uses_into(right, out);
        }
        SsaExpr::Unary { operand, .. } => collect_expr_uses_into(operand, out),
        SsaExpr::Call { args, .. } => {
            for a in args {
                collect_expr_uses_into(a, out);
            }
        }
        SsaExpr::Index { base, index } => {
            collect_expr_uses_into(base, out);
            collect_expr_uses_into(index, out);
        }
        SsaExpr::Member { base, .. } => collect_expr_uses_into(base, out),
        SsaExpr::Cast { expr, .. } => collect_expr_uses_into(expr, out),
        SsaExpr::Convert { value, .. } | SsaExpr::IsType { value, .. } => {
            collect_expr_uses_into(value, out);
        }
        SsaExpr::NewArray { length, .. } => collect_expr_uses_into(length, out),
        SsaExpr::Array(els) => els.iter().for_each(|e| collect_expr_uses_into(e, out)),
        SsaExpr::Struct(elements) => elements
            .iter()
            .for_each(|element| collect_expr_uses_into(element, out)),
        SsaExpr::Map(pairs) => pairs.iter().for_each(|(k, v)| {
            collect_expr_uses_into(k, out);
            collect_expr_uses_into(v, out);
        }),
        SsaExpr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_uses_into(condition, out);
            collect_expr_uses_into(then_expr, out);
            collect_expr_uses_into(else_expr, out);
        }
        SsaExpr::Literal(_) => {}
    }
}
