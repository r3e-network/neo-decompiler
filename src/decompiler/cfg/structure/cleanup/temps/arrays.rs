//! Array-constructor and SETITEM folding for structured IR.

use crate::decompiler::ir::{Block as IrBlock, Expr, Intrinsic, Literal, SemanticCallTarget, Stmt};
use crate::instruction::OpCode;
use std::collections::BTreeSet;

use super::queries::expr_mentions_any;
use super::support::for_each_child_block_mut;

// Array initializer folding
// ---------------------------------------------------------------------------

/// Fold `t = new T[n]; t[0] = v0; ...; t[n-1] = v(n-1);` into a single array
/// literal assignment. Neo's C# compiler emits the constructor+SETITEM shape
/// for every array literal, so recovering the literal is one of the biggest
/// readability wins in storage-key-heavy contracts.
pub(super) fn fold_array_initializers(block: &mut IrBlock) {
    for statement in &mut block.stmts {
        for_each_child_block_mut(statement, &mut |child| fold_array_initializers(child));
    }

    let mut index = 0;
    while index < block.stmts.len() {
        let Some((target, length, kind)) = new_array_target(&block.stmts[index]) else {
            index += 1;
            continue;
        };
        if length == 0 || length > 64 {
            index += 1;
            continue;
        }

        let mut values: Vec<Expr> = Vec::new();
        let mut cursor = index + 1;
        while values.len() < length && cursor < block.stmts.len() {
            let Stmt::ExprStmt(Expr::Call {
                target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Setitem)),
                args,
            }) = &block.stmts[cursor]
            else {
                break;
            };
            let [receiver, slot, value] = args.as_slice() else {
                break;
            };
            if !matches!(receiver, Expr::Variable(name) if name == &target)
                || const_int(slot) != Some(values.len() as i64)
                || expr_mentions_any(value, &BTreeSet::from([target.clone()]))
            {
                break;
            }
            values.push(value.clone());
            cursor += 1;
        }
        if values.len() != length {
            index += 1;
            continue;
        }

        let folded = match kind {
            ArrayKind::Buffer => {
                // Keep the byte[] type honest: only fold when every element is
                // byte-typed already or a byte-sized constant.
                let mut elements = Vec::with_capacity(values.len());
                for value in values {
                    match &value {
                        Expr::Cast { target_type, .. } if target_type == "byte" => {
                            elements.push(value);
                        }
                        Expr::Literal(Literal::Int(number)) if (0..=255).contains(number) => {
                            elements.push(Expr::Cast {
                                expr: Box::new(Expr::Literal(Literal::Int(*number))),
                                target_type: "byte".to_string(),
                            });
                        }
                        _ => {
                            elements.clear();
                            break;
                        }
                    }
                }
                if elements.is_empty() {
                    index += 1;
                    continue;
                }
                Expr::Array(elements)
            }
            ArrayKind::Struct => Expr::Struct(values),
            ArrayKind::Object => Expr::Array(values),
        };
        block.stmts[index] = Stmt::assign(target, folded);
        block.stmts.drain(index + 1..cursor);
        index += 1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArrayKind {
    Buffer,
    Struct,
    Object,
}

/// Match `t = new <array>[n]` in its two IR spellings (sized-array node and
/// allocation intrinsic), returning the target, element count, and array kind.
fn new_array_target(statement: &Stmt) -> Option<(String, usize, ArrayKind)> {
    let Stmt::Assign { target, value } = statement else {
        return None;
    };
    match value {
        Expr::NewArray {
            length,
            element_type,
        } => {
            let length = const_int(length)?.try_into().ok()?;
            let kind = match element_type {
                Some(crate::decompiler::analysis::types::ValueType::Buffer) => ArrayKind::Buffer,
                Some(crate::decompiler::analysis::types::ValueType::Struct) => ArrayKind::Struct,
                _ => ArrayKind::Object,
            };
            Some((target.clone(), length, kind))
        }
        Expr::Call {
            target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        } => {
            let kind = match opcode {
                OpCode::Newbuffer => ArrayKind::Buffer,
                OpCode::Newstruct => ArrayKind::Struct,
                OpCode::Newarray | OpCode::NewarrayT => ArrayKind::Object,
                _ => return None,
            };
            let length = const_int(args.first()?)?.try_into().ok()?;
            Some((target.clone(), length, kind))
        }
        _ => None,
    }
}

fn const_int(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Literal(Literal::Int(value)) => Some(*value),
        Expr::Cast { expr, target_type } if target_type == "int" => const_int(expr),
        _ => None,
    }
}
