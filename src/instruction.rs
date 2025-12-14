//! Neo VM instruction and operand types.
//!
//! This module defines the [`Instruction`] type returned by the disassembler,
//! along with [`OpCode`] and the supported operand representations.

mod model;
mod opcode;
mod operand;

pub use model::Instruction;
pub use opcode::{OpCode, OperandEncoding};
pub use operand::Operand;
