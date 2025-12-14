//! Tests for CFG construction.

use crate::instruction::{Instruction, OpCode, Operand};

use super::basic_block::{BlockId, Terminator};
use super::builder::CfgBuilder;

fn make_instr(offset: usize, opcode: OpCode, operand: Option<Operand>) -> Instruction {
    Instruction::new(offset, opcode, operand)
}

mod basic;
mod dot;
mod jumps;
mod reachability;
mod rpo;
mod terminators;
mod try_blocks;
