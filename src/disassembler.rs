//! Stateless Neo VM bytecode decoder used by the decompiler and CLI.
//! Converts raw byte buffers into structured instructions with operands.
use crate::error::{DisassemblyError, Result};
use crate::instruction::{Instruction, OpCode};

mod operand;

/// How to handle unknown opcode bytes encountered during disassembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownHandling {
    /// Surface an error as soon as an unknown opcode is encountered.
    Error,
    /// Emit an `Unknown` instruction and continue disassembling subsequent bytes.
    Permit,
}

/// Stateless helper that decodes Neo VM bytecode into structured instructions.
#[derive(Debug, Clone, Copy)]
///
/// The disassembler maintains no state between calls; configuration only
/// controls how unknown opcode bytes are handled.
pub struct Disassembler {
    unknown: UnknownHandling,
}

impl Default for Disassembler {
    fn default() -> Self {
        Self::new()
    }
}

impl Disassembler {
    /// Create a disassembler that permits unknown opcodes.
    ///
    /// Equivalent to `Disassembler::with_unknown_handling(UnknownHandling::Permit)`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            unknown: UnknownHandling::Permit,
        }
    }

    /// Create a disassembler configured with the desired unknown-opcode policy.
    ///
    /// See [`UnknownHandling`] for the available strategies.
    #[must_use]
    pub fn with_unknown_handling(unknown: UnknownHandling) -> Self {
        Self { unknown }
    }

    /// Disassemble an entire bytecode buffer.
    ///
    /// # Errors
    /// Returns an error if the bytecode stream is truncated, contains an operand
    /// that exceeds the supported maximum size, or contains an unknown opcode
    /// while configured with [`UnknownHandling::Error`].
    pub fn disassemble(&self, bytecode: &[u8]) -> Result<Vec<Instruction>> {
        let mut instructions = Vec::new();
        let mut pc = 0usize;

        while pc < bytecode.len() {
            let (instruction, size) = self.decode_instruction(bytecode, pc)?;
            instructions.push(instruction);
            pc += size;
        }

        Ok(instructions)
    }

    fn decode_instruction(&self, bytecode: &[u8], offset: usize) -> Result<(Instruction, usize)> {
        let opcode_byte = *bytecode
            .get(offset)
            .ok_or(DisassemblyError::UnexpectedEof { offset })?;
        let opcode = OpCode::from_byte(opcode_byte);
        if let OpCode::Unknown(_) = opcode {
            return match self.unknown {
                UnknownHandling::Permit => Ok((Instruction::new(offset, opcode, None), 1)),
                UnknownHandling::Error => Err(DisassemblyError::UnknownOpcode {
                    opcode: opcode_byte,
                    offset,
                }
                .into()),
            };
        }

        let (operand, consumed) = self.read_operand(opcode, bytecode, offset)?;
        Ok((Instruction::new(offset, opcode, operand), 1 + consumed))
    }
}

#[cfg(test)]
mod tests;
