use super::{OpCode, Operand};

/// A decoded Neo VM instruction with its bytecode offset.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Instruction {
    /// Offset of the opcode byte within the original bytecode buffer.
    pub offset: usize,
    /// Decoded opcode.
    pub opcode: OpCode,
    /// Decoded operand, if the opcode carries one.
    pub operand: Option<Operand>,
}

impl Instruction {
    /// Construct an instruction at the given bytecode offset.
    ///
    /// Most users will obtain instructions from [`crate::Disassembler`] or
    /// [`crate::Decompiler`], but this constructor is useful when creating
    /// synthetic instruction streams for tests.
    ///
    /// # Examples
    /// ```
    /// use neo_decompiler::{Instruction, OpCode, Operand};
    ///
    /// let ins = Instruction::new(0, OpCode::Push0, Some(Operand::I32(0)));
    /// assert_eq!(ins.offset, 0);
    /// assert_eq!(ins.opcode, OpCode::Push0);
    /// ```
    #[must_use]
    pub fn new(offset: usize, opcode: OpCode, operand: Option<Operand>) -> Self {
        Self {
            offset,
            opcode,
            operand,
        }
    }
}
