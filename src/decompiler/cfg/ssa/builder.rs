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
    pub fn build(self) -> SsaForm {
        // For now, create a minimal SSA form
        // Full implementation requires IR generation from instructions
        let mut ssa_blocks = BTreeMap::new();

        for block in self.cfg.blocks() {
            let ssa_block = SsaBlock::new();
            ssa_blocks.insert(block.id, ssa_block);
        }

        SsaForm {
            cfg: self.cfg.clone(),
            dominance: self.dominance,
            blocks: ssa_blocks,
            definitions: BTreeMap::new(),
            uses: BTreeMap::new(),
        }
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
}
