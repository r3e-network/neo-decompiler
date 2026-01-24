//! SSA construction from a CFG and instruction stream.
//!
//! Implements the two-phase SSA construction algorithm:
//! 1. φ node insertion using dominance frontiers
//! 2. Variable renaming via dominator tree traversal

use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::cfg::{BlockId, Cfg};
use crate::instruction::Instruction;

use super::dominance::{self, DominanceInfo};
use super::form::{SsaBlock, SsaForm};
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

        for block in self.cfg.blocks() {
            let mut ssa_block = SsaBlock::new();

            // Add φ nodes for this block (if any)
            if let Some(_locations) = self.phi_locations.get("") {
                // φ nodes will be added here in future implementation
            }

            // For now, just create empty blocks
            // Full implementation will convert instructions to SSA statements
            ssa_blocks.insert(block.id, ssa_block);
        }

        SsaForm {
            cfg: self.cfg.clone(),
            dominance: self.dominance.clone(),
            blocks: ssa_blocks,
            definitions: BTreeMap::new(),
            uses: BTreeMap::new(),
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
        let block = BasicBlock::new(
            BlockId::ENTRY,
            0,
            0,
            0..0,
            Terminator::Return,
        );
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
        assert!(builder.version_stack.get("x").map(|v| v.is_empty()).unwrap_or(true));
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
                    Terminator::Jump { target: BlockId(i + 1) }
                } else {
                    Terminator::Return
                },
            );
            cfg.add_block(block);

            if i > 0 {
                cfg.add_edge(BlockId(i - 1), BlockId(i), crate::decompiler::cfg::EdgeKind::Unconditional);
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
