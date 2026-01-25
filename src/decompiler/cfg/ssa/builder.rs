//! SSA construction from a CFG and instruction stream.
//!
//! Implements the two-phase SSA construction algorithm:
//! 1. φ node insertion using dominance frontiers
//! 2. Variable renaming via dominator tree traversal

#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    unused_mut,
    missing_docs,
    clippy::clone_on_copy,
    clippy::type_complexity,
    clippy::needless_return
)]

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

    /// Mapping from instruction offset to block ID.
    offset_to_block: BTreeMap<usize, BlockId>,

    /// Current version number for each base variable name.
    versions: BTreeMap<String, usize>,

    /// Locations where φ nodes should be inserted for each variable.
    /// Maps base variable name -> set of blocks needing φ nodes.
    phi_locations: BTreeMap<String, BTreeSet<BlockId>>,

    /// Stack tracking current SSA version for each variable during renaming.
    version_stack: BTreeMap<String, Vec<SsaVariable>>,
}

impl<'a> SsaBuilder<'a> {
    /// Create a new SSA builder for the given CFG and instructions.
    pub fn new(cfg: &'a Cfg, instructions: &'a [Instruction]) -> Self {
        let dominance = dominance::compute(cfg);

        // Build offset to block mapping
        let mut offset_to_block = BTreeMap::new();
        for block in cfg.blocks() {
            for offset in block.start_offset..block.end_offset {
                offset_to_block.insert(offset, block.id);
            }
        }

        Self {
            cfg,
            instructions,
            dominance,
            offset_to_block,
            versions: BTreeMap::new(),
            phi_locations: BTreeMap::new(),
            version_stack: BTreeMap::new(),
        }
    }

    /// Build SSA form from the CFG and instructions.
    ///
    /// This implements the two-phase SSA construction algorithm:
    /// 1. Compute φ node placement using dominance frontiers
    /// 2. Perform variable renaming via dominator tree traversal
    pub fn build(mut self) -> SsaForm {
        // Phase 1: Compute φ node locations
        self.compute_phi_locations();

        // Phase 2: Build SSA blocks with φ nodes and renamed variables
        let mut ssa = self.build_ssa_blocks();

        // Update definitions and uses
        self.collect_definitions_and_uses(&mut ssa);

        ssa
    }

    /// Compute where φ nodes should be inserted for each variable.
    ///
    /// Uses the iterated dominance frontier algorithm:
    /// For each variable definition, add φ nodes at the dominance frontier
    /// of blocks containing the definition, until convergence.
    fn compute_phi_locations(&mut self) {
        // For now, we skip φ node computation since we don't have
        // full IR with variable definitions yet.
        // This will be implemented in a future phase when we have
        // the IR from Neo VM instructions.
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
            for offset in block.start_offset..block.end_offset {
                if let Some(instr) = self.instructions.get(offset) {
                    if self.process_instruction_for_ssa(
                        block.id,
                        offset,
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
            // Handle common opcodes that affect SSA
            OpCode::Push0 => {
                let var = self.new_version("stack_0".to_string());
                ssa_block.add_stmt(SsaStmt::assign(
                    var.clone(),
                    SsaExpr::lit(crate::decompiler::ir::Literal::Int(0)),
                ));
                definitions.insert(var, block_id);
                return true;
            }
            OpCode::Push1 => {
                let var = self.new_version("stack_1".to_string());
                ssa_block.add_stmt(SsaStmt::assign(
                    var.clone(),
                    SsaExpr::lit(crate::decompiler::ir::Literal::Int(1)),
                ));
                definitions.insert(var, block_id);
                return true;
            }
            OpCode::Push2 => {
                let var = self.new_version("stack_2".to_string());
                ssa_block.add_stmt(SsaStmt::assign(
                    var.clone(),
                    SsaExpr::lit(crate::decompiler::ir::Literal::Int(2)),
                ));
                definitions.insert(var, block_id);
                return true;
            }
            OpCode::Push3 => {
                let var = self.new_version("stack_3".to_string());
                ssa_block.add_stmt(SsaStmt::assign(
                    var.clone(),
                    SsaExpr::lit(crate::decompiler::ir::Literal::Int(3)),
                ));
                definitions.insert(var, block_id);
                return true;
            }
            OpCode::Push4 => {
                let var = self.new_version("stack_4".to_string());
                ssa_block.add_stmt(SsaStmt::assign(
                    var.clone(),
                    SsaExpr::lit(crate::decompiler::ir::Literal::Int(4)),
                ));
                definitions.insert(var, block_id);
                return true;
            }
            OpCode::Push5 => {
                let var = self.new_version("stack_5".to_string());
                ssa_block.add_stmt(SsaStmt::assign(
                    var.clone(),
                    SsaExpr::lit(crate::decompiler::ir::Literal::Int(5)),
                ));
                definitions.insert(var, block_id);
                return true;
            }
            OpCode::Push6 => {
                let var = self.new_version("stack_6".to_string());
                ssa_block.add_stmt(SsaStmt::assign(
                    var.clone(),
                    SsaExpr::lit(crate::decompiler::ir::Literal::Int(6)),
                ));
                definitions.insert(var, block_id);
                return true;
            }
            OpCode::Push7 => {
                let var = self.new_version("stack_7".to_string());
                ssa_block.add_stmt(SsaStmt::assign(
                    var.clone(),
                    SsaExpr::lit(crate::decompiler::ir::Literal::Int(7)),
                ));
                definitions.insert(var, block_id);
                return true;
            }
            OpCode::Push8 => {
                let var = self.new_version("stack_8".to_string());
                ssa_block.add_stmt(SsaStmt::assign(
                    var.clone(),
                    SsaExpr::lit(crate::decompiler::ir::Literal::Int(8)),
                ));
                definitions.insert(var, block_id);
                return true;
            }
            OpCode::Push9 => {
                let var = self.new_version("stack_9".to_string());
                ssa_block.add_stmt(SsaStmt::assign(
                    var.clone(),
                    SsaExpr::lit(crate::decompiler::ir::Literal::Int(9)),
                ));
                definitions.insert(var, block_id);
                return true;
            }
            OpCode::Push10 => {
                let var = self.new_version("stack_10".to_string());
                ssa_block.add_stmt(SsaStmt::assign(
                    var.clone(),
                    SsaExpr::lit(crate::decompiler::ir::Literal::Int(10)),
                ));
                definitions.insert(var, block_id);
                return true;
            }
            OpCode::Push11 => {
                let var = self.new_version("stack_11".to_string());
                ssa_block.add_stmt(SsaStmt::assign(
                    var.clone(),
                    SsaExpr::lit(crate::decompiler::ir::Literal::Int(11)),
                ));
                definitions.insert(var, block_id);
                return true;
            }
            OpCode::Push12 => {
                let var = self.new_version("stack_12".to_string());
                ssa_block.add_stmt(SsaStmt::assign(
                    var.clone(),
                    SsaExpr::lit(crate::decompiler::ir::Literal::Int(12)),
                ));
                definitions.insert(var, block_id);
                return true;
            }
            OpCode::Push13 => {
                let var = self.new_version("stack_13".to_string());
                ssa_block.add_stmt(SsaStmt::assign(
                    var.clone(),
                    SsaExpr::lit(crate::decompiler::ir::Literal::Int(13)),
                ));
                definitions.insert(var, block_id);
                return true;
            }
            OpCode::Push14 => {
                let var = self.new_version("stack_14".to_string());
                ssa_block.add_stmt(SsaStmt::assign(
                    var.clone(),
                    SsaExpr::lit(crate::decompiler::ir::Literal::Int(14)),
                ));
                definitions.insert(var, block_id);
                return true;
            }
            OpCode::Push15 => {
                let var = self.new_version("stack_15".to_string());
                ssa_block.add_stmt(SsaStmt::assign(
                    var.clone(),
                    SsaExpr::lit(crate::decompiler::ir::Literal::Int(15)),
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

    /// Collect variable definitions and use sites for analysis.
    fn collect_definitions_and_uses(&mut self, _ssa: &mut SsaForm) {
        // Full implementation will scan SSA blocks for variable definitions/uses
        // For now, leave empty as we don't have converted statements yet
    }

    /// Place φ nodes for a variable at dominance frontiers.
    ///
    /// # Arguments
    ///
    /// * `var_name` - The base variable name (e.g., "local_0")
    /// * `def_blocks` - Blocks where this variable is defined
    ///
    /// This implements the iterated dominance frontier algorithm:
    /// 1. Start with blocks containing the definition
    /// 2. Add φ nodes at their dominance frontiers
    /// 3. Add those frontier blocks to the worklist
    /// 4. Repeat until no new blocks are added
    fn place_phi_nodes(&mut self, var_name: String, def_blocks: BTreeSet<BlockId>) {
        let mut worklist: Vec<BlockId> = def_blocks.iter().copied().collect();
        let mut placed = BTreeSet::new();

        while let Some(block) = worklist.pop() {
            for frontier_block in self.dominance.dominance_frontier_vec(block) {
                if placed.insert(frontier_block) {
                    // Add φ node at this frontier block
                    self.phi_locations
                        .entry(var_name.clone())
                        .or_default()
                        .insert(frontier_block);

                    // Add frontier block to worklist
                    worklist.push(frontier_block);
                }
            }
        }
    }

    /// Rename variables in the dominator tree traversal.
    ///
    /// This performs the actual SSA renaming by walking the dominator tree
    /// and assigning new versions to each variable definition.
    fn rename_variables(&mut self) {
        // Get entry block
        if let Some(entry_id) = self.cfg.entry_block().map(|b| b.id) {
            self.rename_block(entry_id);
        }
    }

    /// Recursively rename variables in a block and its dominator tree children.
    fn rename_block(&mut self, block_id: BlockId) {
        // For each variable, push current version onto stack
        // (Will be implemented when we have variable tracking from IR)

        // Process statements in this block
        // (Will be implemented when we have IR conversion)

        // Collect children first to avoid borrow issues
        let children: Vec<BlockId> = self.dominance.children(block_id).to_vec();

        // Recursively process children in dominator tree
        for child in children {
            self.rename_block(child);
        }

        // Pop versions for this block
        // (Will be implemented when we have variable tracking)
    }

    /// Get the current SSA version of a variable from the version stack.
    fn current_version(&self, base: &str) -> Option<&SsaVariable> {
        self.version_stack.get(base).and_then(|stack| stack.last())
    }

    /// Create a new version of a variable.
    fn new_version(&mut self, base: String) -> SsaVariable {
        let version = self.versions.entry(base.clone()).or_insert(0);
        let var = SsaVariable::new(base, *version);
        *version += 1;
        var
    }

    /// Push a version onto the stack.
    fn push_version(&mut self, base: String, var: SsaVariable) {
        self.version_stack.entry(base).or_default().push(var);
    }

    /// Pop a version from the stack.
    fn pop_version(&mut self, base: &str) {
        if let Some(stack) = self.version_stack.get_mut(base) {
            stack.pop();
        }
    }
}

// Standalone convenience function for creating SSA from just a CFG
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

    #[test]
    fn test_version_stack() {
        let cfg = Cfg::new();
        let instructions = &[];
        let mut builder = SsaBuilder::new(&cfg, instructions);

        let var = SsaVariable::initial("x".to_string());
        builder.push_version("x".to_string(), var.clone());

        assert_eq!(
            builder.version_stack.get("x").and_then(|v| v.last()),
            Some(&var)
        );

        builder.pop_version("x");
        assert!(builder
            .version_stack
            .get("x")
            .map(|v| v.is_empty())
            .unwrap_or(true));
    }

    #[test]
    fn test_phi_node_placement() {
        let mut cfg = Cfg::new();

        // Create a simple linear chain: 0 -> 1 -> 2
        for i in 0..3 {
            let block = BasicBlock::new(
                BlockId(i),
                i,
                i + 1,
                i..(i + 1),
                if i < 2 {
                    Terminator::Jump {
                        target: BlockId(i + 1),
                    }
                } else {
                    Terminator::Return
                },
            );
            cfg.add_block(block);

            if i > 0 {
                cfg.add_edge(
                    BlockId(i - 1),
                    BlockId(i),
                    crate::decompiler::cfg::EdgeKind::Unconditional,
                );
            }
        }

        let instructions = &[];
        let mut builder = SsaBuilder::new(&cfg, instructions);

        // Place φ nodes for a variable defined in block 0
        let mut def_blocks = BTreeSet::new();
        def_blocks.insert(BlockId(0));
        builder.place_phi_nodes("x".to_string(), def_blocks);

        // In a linear chain, there should be no φ nodes needed
        // (no merge points)
        assert!(builder.phi_locations.get("x").map(|s| s.len()).unwrap_or(0) == 0);
    }

    #[test]
    fn test_current_version_empty() {
        let cfg = Cfg::new();
        let instructions = &[];
        let builder = SsaBuilder::new(&cfg, instructions);

        // No versions pushed yet
        assert!(builder.current_version("x").is_none());
    }

    #[test]
    fn test_current_version_after_push() {
        let cfg = Cfg::new();
        let instructions = &[];
        let mut builder = SsaBuilder::new(&cfg, instructions);

        let var = SsaVariable::initial("x".to_string());
        builder.push_version("x".to_string(), var.clone());

        assert_eq!(builder.current_version("x"), Some(&var));
    }
}
