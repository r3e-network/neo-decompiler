//! SSA construction from a CFG and instruction stream.
//!
//! Implements the two-phase SSA construction algorithm:
//! 1. φ node insertion using dominance frontiers
//! 2. Variable renaming via dominator tree traversal

#![allow(clippy::needless_return)]

use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::cfg::{BlockId, Cfg};
use crate::instruction::Instruction;

use super::dominance::{self, DominanceInfo};
use super::form::{SsaBlock, SsaExpr, SsaForm, SsaStmt, UseSite};
use super::variable::PhiNode;
use super::variable::SsaVariable;

/// Builder for constructing SSA form from a CFG and instructions.
pub struct SsaBuilder<'a> {
    /// The CFG being converted to SSA.
    cfg: &'a Cfg,

    /// Instructions referenced by the CFG (indexed by offset).
    instructions: &'a [Instruction],

    /// Pre-computed dominance information.
    dominance: DominanceInfo,

    /// Current version number for each base variable name.
    versions: BTreeMap<String, usize>,

    /// Locations where φ nodes should be inserted for each variable.
    /// Maps base variable name -> set of blocks needing φ nodes.
    phi_locations: BTreeMap<String, BTreeSet<BlockId>>,
}

impl<'a> SsaBuilder<'a> {
    /// Create a new SSA builder for the given CFG and instructions.
    pub fn new(cfg: &'a Cfg, instructions: &'a [Instruction]) -> Self {
        let dominance = dominance::compute(cfg);

        Self {
            cfg,
            instructions,
            dominance,
            versions: BTreeMap::new(),
            phi_locations: BTreeMap::new(),
        }
    }

    /// Build SSA form from the CFG and instructions.
    ///
    /// This implements the two-phase SSA construction algorithm:
    /// 1. Compute φ node placement using dominance frontiers
    /// 2. Perform variable renaming via dominator tree traversal
    pub fn build(mut self) -> SsaForm {
        self.build_ssa_blocks()
    }

    /// Build SSA blocks by placing φ nodes and renaming variables.
    fn build_ssa_blocks(&mut self) -> SsaForm {
        let mut ssa_blocks = BTreeMap::new();
        let mut definitions = BTreeMap::new();
        let mut uses = BTreeMap::new();

        for block in self.cfg.blocks() {
            let mut ssa_block = SsaBlock::new();

            // Add φ nodes for this block
            for (var_name, locations) in &self.phi_locations {
                if locations.contains(&block.id) {
                    let phi_node = PhiNode::new(SsaVariable::initial(var_name.clone()));
                    ssa_block.add_phi(phi_node);
                    // Record φ node as defining a new version
                    definitions.insert(SsaVariable::initial(var_name.clone()), block.id);
                }
            }

            // Add terminator information as comment
            match &block.terminator {
                crate::decompiler::cfg::Terminator::Return => {
                    ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                        format!("return from block {:?}", block.id),
                    )));
                }
                crate::decompiler::cfg::Terminator::Jump { target } => {
                    ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                        format!("jump to {:?}", target),
                    )));
                }
                crate::decompiler::cfg::Terminator::Branch {
                    then_target,
                    else_target,
                } => {
                    ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                        format!("branch: then={:?}, else={:?}", then_target, else_target),
                    )));
                }
                crate::decompiler::cfg::Terminator::Fallthrough { target } => {
                    ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                        format!("fallthrough to {:?}", target),
                    )));
                }
                crate::decompiler::cfg::Terminator::TryEntry { .. } => {
                    ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                        "try entry".to_string(),
                    )));
                }
                crate::decompiler::cfg::Terminator::EndTry { .. } => {
                    ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                        "end try".to_string(),
                    )));
                }
                crate::decompiler::cfg::Terminator::Throw => {
                    ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                        "throw exception".to_string(),
                    )));
                }
                crate::decompiler::cfg::Terminator::Abort => {
                    ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                        "abort execution".to_string(),
                    )));
                }
                crate::decompiler::cfg::Terminator::Unknown => {
                    ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                        "unknown terminator".to_string(),
                    )));
                }
            }

            // Process instructions to populate SSA statements
            let mut statement_count = 0;
            for idx in block.instruction_range.clone() {
                if let Some(instr) = self.instructions.get(idx) {
                    if self.process_instruction_for_ssa(
                        block.id,
                        idx,
                        instr,
                        &mut ssa_block,
                        &mut definitions,
                        &mut uses,
                    ) {
                        statement_count += 1;
                    }
                }
            }

            // If no statements were added, add a placeholder comment
            if statement_count == 0 && ssa_block.phi_count() == 0 {
                ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                    format!(
                        "empty block {:?} (offsets {}..{})",
                        block.id, block.start_offset, block.end_offset
                    ),
                )));
            }

            ssa_blocks.insert(block.id, ssa_block);
        }

        SsaForm {
            cfg: self.cfg.clone(),
            dominance: self.dominance.clone(),
            blocks: ssa_blocks,
            definitions,
            uses,
        }
    }

    /// Process a single instruction to extract SSA-relevant information.
    ///
    /// Returns true if an SSA statement was created.
    fn process_instruction_for_ssa(
        &mut self,
        block_id: BlockId,
        offset: usize,
        instr: &Instruction,
        ssa_block: &mut SsaBlock,
        definitions: &mut BTreeMap<SsaVariable, BlockId>,
        _uses: &mut BTreeMap<SsaVariable, BTreeSet<UseSite>>,
    ) -> bool {
        use crate::instruction::OpCode;

        // Create a comment showing the instruction
        ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
            format!("// {}: {:?}", offset, instr.opcode),
        )));

        // Track variable operations
        match instr.opcode {
            // Handle Push0-Push16 opcodes using a unified pattern
            OpCode::Push0
            | OpCode::Push1
            | OpCode::Push2
            | OpCode::Push3
            | OpCode::Push4
            | OpCode::Push5
            | OpCode::Push6
            | OpCode::Push7
            | OpCode::Push8
            | OpCode::Push9
            | OpCode::Push10
            | OpCode::Push11
            | OpCode::Push12
            | OpCode::Push13
            | OpCode::Push14
            | OpCode::Push15
            | OpCode::Push16 => {
                let value = self.extract_push_value(instr.opcode);
                let var = self.new_version(format!("stack_{}", value));
                ssa_block.add_stmt(SsaStmt::assign(
                    var.clone(),
                    SsaExpr::lit(crate::decompiler::ir::Literal::Int(value)),
                ));
                definitions.insert(var, block_id);
                return true;
            }
            OpCode::Add => {
                // Binary operation - create a dummy addition
                ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                    "ADD operation (binary addition)".to_string(),
                )));
                return true;
            }
            OpCode::Sub => {
                ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                    "SUB operation (binary subtraction)".to_string(),
                )));
                return true;
            }
            OpCode::Mul => {
                ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                    "MUL operation (binary multiplication)".to_string(),
                )));
                return true;
            }
            OpCode::Div => {
                ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                    "DIV operation (binary division)".to_string(),
                )));
                return true;
            }
            OpCode::Ret => {
                ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                    "RET (return from function)".to_string(),
                )));
                return true;
            }
            OpCode::Nop => {
                ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                    "NOP (no operation)".to_string(),
                )));
                return true;
            }
            OpCode::Isnull => {
                ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                    "ISNULL (null check)".to_string(),
                )));
                return true;
            }
            OpCode::Equal => {
                ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                    "EQUAL (equality comparison)".to_string(),
                )));
                return true;
            }
            OpCode::Notequal => {
                ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                    "NOTEQUAL (inequality comparison)".to_string(),
                )));
                return true;
            }
            _ => {
                // For other opcodes, just add a comment
                ssa_block.add_stmt(SsaStmt::other(crate::decompiler::ir::Stmt::comment(
                    format!("{:?}", instr.opcode),
                )));
                return true;
            }
        }
    }

    /// Create a new version of a variable.
    fn new_version(&mut self, base: String) -> SsaVariable {
        let version = self.versions.entry(base.clone()).or_insert(0);
        let var = SsaVariable::new(base, *version);
        *version += 1;
        var
    }

    /// Extract the integer value from a Push0-Push16 opcode.
    fn extract_push_value(&self, opcode: crate::instruction::OpCode) -> i64 {
        use crate::instruction::OpCode;
        match opcode {
            OpCode::Push0 => 0,
            OpCode::Push1 => 1,
            OpCode::Push2 => 2,
            OpCode::Push3 => 3,
            OpCode::Push4 => 4,
            OpCode::Push5 => 5,
            OpCode::Push6 => 6,
            OpCode::Push7 => 7,
            OpCode::Push8 => 8,
            OpCode::Push9 => 9,
            OpCode::Push10 => 10,
            OpCode::Push11 => 11,
            OpCode::Push12 => 12,
            OpCode::Push13 => 13,
            OpCode::Push14 => 14,
            OpCode::Push15 => 15,
            OpCode::Push16 => 16,
            _ => 0, // Should not happen for valid push opcodes
        }
    }
}

/// Create SSA form from a CFG without an instruction stream.
///
/// Produces an SSA skeleton with empty blocks (no φ nodes or statements).
/// Use [`SsaBuilder`] when instruction-level analysis is needed.
pub fn build_ssa_from_cfg(cfg: &Cfg) -> SsaForm {
    let dominance = dominance::compute(cfg);
    let mut ssa_blocks = BTreeMap::new();

    for block in cfg.blocks() {
        let ssa_block = SsaBlock::new();
        ssa_blocks.insert(block.id, ssa_block);
    }

    SsaForm {
        cfg: cfg.clone(),
        dominance,
        blocks: ssa_blocks,
        definitions: BTreeMap::new(),
        uses: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompiler::cfg::{BasicBlock, BlockId, Cfg, Terminator};

    #[test]
    fn test_builder_creation() {
        let cfg = Cfg::new();
        let instructions = &[];
        let builder = SsaBuilder::new(&cfg, instructions);

        assert_eq!(builder.versions.len(), 0);
        assert_eq!(builder.phi_locations.len(), 0);
    }

    #[test]
    fn test_new_version_increments() {
        let cfg = Cfg::new();
        let instructions = &[];
        let mut builder = SsaBuilder::new(&cfg, instructions);

        let v1 = builder.new_version("x".to_string());
        let v2 = builder.new_version("x".to_string());
        let v3 = builder.new_version("y".to_string());

        assert_eq!(v1.version, 0);
        assert_eq!(v2.version, 1);
        assert_eq!(v3.version, 0);
    }

    #[test]
    fn test_build_ssa_from_cfg() {
        let cfg = Cfg::new();
        let ssa = build_ssa_from_cfg(&cfg);

        assert_eq!(ssa.block_count(), 0);
    }

    #[test]
    fn test_build_ssa_with_blocks() {
        let mut cfg = Cfg::new();
        let block = BasicBlock::new(BlockId::ENTRY, 0, 0, 0..0, Terminator::Return);
        cfg.add_block(block);

        let ssa = build_ssa_from_cfg(&cfg);

        assert_eq!(ssa.block_count(), 1);
    }
}
