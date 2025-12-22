//! Stateless Neo VM bytecode decoder used by the decompiler and CLI.
//! Converts raw byte buffers into structured instructions with operands.
use std::fmt;

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

/// Disassembly output including any non-fatal warnings.
#[derive(Debug, Clone)]
pub struct DisassemblyOutput {
    /// Decoded instructions.
    pub instructions: Vec<Instruction>,
    /// Non-fatal warnings encountered during decoding.
    pub warnings: Vec<DisassemblyWarning>,
}

/// Warning emitted during disassembly when configured to tolerate issues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisassemblyWarning {
    /// An unknown opcode was encountered; output may be desynchronized.
    UnknownOpcode {
        /// The raw opcode byte.
        opcode: u8,
        /// Offset where the opcode byte was encountered.
        offset: usize,
    },
}

impl fmt::Display for DisassemblyWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DisassemblyWarning::UnknownOpcode { opcode, offset } => write!(
                f,
                "disassembly: unknown opcode 0x{opcode:02X} at 0x{offset:04X}; continuing may desynchronize output"
            ),
        }
    }
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
    ///
    /// Any non-fatal warnings are discarded; call [`Self::disassemble_with_warnings`]
    /// to inspect them.
    pub fn disassemble(&self, bytecode: &[u8]) -> Result<Vec<Instruction>> {
        Ok(self.disassemble_with_warnings(bytecode)?.instructions)
    }

    /// Disassemble an entire bytecode buffer, returning any non-fatal warnings.
    ///
    /// # Errors
    /// Returns an error if the bytecode stream is truncated, contains an operand
    /// that exceeds the supported maximum size, or contains an unknown opcode
    /// while configured with [`UnknownHandling::Error`].
    pub fn disassemble_with_warnings(&self, bytecode: &[u8]) -> Result<DisassemblyOutput> {
        let mut instructions = Vec::new();
        let mut warnings = Vec::new();
        let mut pc = 0usize;

        while pc < bytecode.len() {
            let opcode_byte = *bytecode
                .get(pc)
                .ok_or(DisassemblyError::UnexpectedEof { offset: pc })?;
            let opcode = OpCode::from_byte(opcode_byte);
            if let OpCode::Unknown(_) = opcode {
                match self.unknown {
                    UnknownHandling::Permit => {
                        warnings.push(DisassemblyWarning::UnknownOpcode {
                            opcode: opcode_byte,
                            offset: pc,
                        });
                        instructions.push(Instruction::new(pc, opcode, None));
                        pc += 1;
                        continue;
                    }
                    UnknownHandling::Error => {
                        return Err(DisassemblyError::UnknownOpcode {
                            opcode: opcode_byte,
                            offset: pc,
                        }
                        .into());
                    }
                }
            }

            let (instruction, size) = self.decode_known_instruction(bytecode, pc, opcode)?;
            instructions.push(instruction);
            pc += size;
        }

        Ok(DisassemblyOutput {
            instructions,
            warnings,
        })
    }

    fn decode_known_instruction(
        &self,
        bytecode: &[u8],
        offset: usize,
        opcode: OpCode,
    ) -> Result<(Instruction, usize)> {
        let (operand, consumed) = self.read_operand(opcode, bytecode, offset)?;
        Ok((Instruction::new(offset, opcode, operand), 1 + consumed))
    }
}

#[cfg(test)]
mod tests;
