use super::*;
use crate::decompiler::cfg::method_body::{Fidelity, LoweringIssueKind};
use crate::decompiler::cfg::ssa::{CallContract, MethodContext};
use crate::decompiler::cfg::{BasicBlock, BlockId, CfgBuilder, EdgeKind, Terminator};
use crate::decompiler::ir::{Intrinsic, SemanticCallTarget};
use crate::instruction::{Instruction, OpCode, Operand};

/// Build instructions + a matching CFG for a straight-line program.
fn linear(instrs: Vec<Instruction>) -> (Vec<Instruction>, Cfg) {
    let cfg = CfgBuilder::new(&instrs).build();
    (instrs, cfg)
}

fn instr(off: usize, op: OpCode) -> Instruction {
    Instruction::new(off, op, None)
}

fn uneven_stack_merge(tail: Vec<Instruction>) -> (Vec<Instruction>, Cfg) {
    let mut instructions = vec![
        instr(0, OpCode::Nop),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Nop),
    ];
    instructions.extend(tail);

    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        1,
        0..1,
        Terminator::Branch {
            then_target: BlockId(1),
            else_target: BlockId(2),
        },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(1),
        1,
        2,
        1..2,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(2),
        2,
        3,
        2..3,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(3),
        3,
        instructions.len(),
        3..instructions.len(),
        Terminator::Return,
    ));
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);

    (instructions, cfg)
}

fn first_nonliteral_assignment(instructions: &[Instruction]) -> SsaExpr {
    let cfg = CfgBuilder::new(instructions).build();
    SsaBuilder::new(&cfg, instructions)
        .build()
        .blocks_iter()
        .flat_map(|(_, block)| &block.stmts)
        .find_map(|statement| match statement {
            SsaStmt::Assign { value, .. } if !matches!(value, SsaExpr::Literal(_)) => {
                Some(value.clone())
            }
            _ => None,
        })
        .expect("program should produce a non-literal assignment")
}

fn optimized_return_expression(instructions: &[Instruction]) -> SsaExpr {
    let cfg = CfgBuilder::new(instructions).build();
    let mut ssa = SsaBuilder::new(&cfg, instructions).build();
    crate::decompiler::cfg::ssa::optimize_ssa(&mut ssa);
    let returned = ssa
        .blocks_iter()
        .flat_map(|(_, block)| &block.stmts)
        .find_map(|statement| match statement {
            SsaStmt::Return(Some(value)) => Some(value.clone()),
            _ => None,
        })
        .expect("program should return a value");
    returned
}

fn optimized_collection_expression(instructions: &[Instruction]) -> SsaExpr {
    let cfg = CfgBuilder::new(instructions).build();
    let mut ssa = SsaBuilder::new(&cfg, instructions).build();
    crate::decompiler::cfg::ssa::optimize_ssa(&mut ssa);
    let collection = ssa
        .blocks_iter()
        .flat_map(|(_, block)| &block.stmts)
        .find_map(|statement| match statement {
            SsaStmt::Assign { value, .. }
                if matches!(
                    value,
                    SsaExpr::Array(_) | SsaExpr::Struct(_) | SsaExpr::Map(_)
                ) =>
            {
                Some(value.clone())
            }
            _ => None,
        })
        .expect("program should construct a collection");
    collection
}

fn has_unpack_packstruct_intrinsic(form: &SsaForm) -> bool {
    form.blocks_iter()
        .flat_map(|(_, block)| &block.stmts)
        .any(|statement| {
            matches!(
                statement,
                SsaStmt::Assign {
                    value: SsaExpr::Call {
                        target: SemanticCallTarget::Intrinsic(Intrinsic::UnpackPackStruct),
                        ..
                    },
                    ..
                }
            )
        })
}

fn has_payloadless_throw(form: &SsaForm) -> bool {
    form.blocks_iter()
        .flat_map(|(_, block)| &block.stmts)
        .any(|statement| matches!(statement, SsaStmt::Throw(None)))
}

mod calls;
mod collection_facts;
mod control_flow;
mod dynamic_stack;
mod fidelity;
mod stack_ops;
