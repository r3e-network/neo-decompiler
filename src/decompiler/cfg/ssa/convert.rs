//! Conversion from IR to SSA form.
//!
//! This module handles the conversion of IR expressions and statements
//! into their SSA equivalents, handling variable versioning and φ nodes.

use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::cfg::{BlockId, Cfg};
use crate::decompiler::ir::{Expr, Literal, Stmt};

use super::dominance::DominanceInfo;
use super::form::{SsaBlock, SsaExpr, SsaForm, SsaStmt};
use super::variable::SsaVariable;

/// Converts IR expressions and statements to SSA form.
pub struct IrToSsaConverter<'a> {
    /// The CFG being converted.
    cfg: &'a Cfg,

    /// Dominance information for φ node placement.
    dominance: DominanceInfo,

    /// Current version number for each base variable name.
    versions: BTreeMap<String, usize>,

    /// Stack tracking current SSA version for each variable during renaming.
    version_stack: BTreeMap<String, Vec<SsaVariable>>,

    /// Locations where φ nodes should be inserted for each variable.
    phi_locations: BTreeMap<String, BTreeSet<BlockId>>,

    /// SSA blocks being built.
    ssa_blocks: BTreeMap<BlockId, SsaBlock>,

    /// Variable definitions tracking.
    definitions: BTreeMap<SsaVariable, BlockId>,

    /// Variable use sites tracking.
    uses: BTreeMap<SsaVariable, BTreeSet<super::form::UseSite>>,
}

impl<'a> IrToSsaConverter<'a> {
    /// Create a new IR to SSA converter.
    pub fn new(cfg: &'a Cfg) -> Self {
        let dominance = super::dominance::compute(cfg);

        Self {
            cfg,
            dominance,
            versions: BTreeMap::new(),
            version_stack: BTreeMap::new(),
            phi_locations: BTreeMap::new(),
            ssa_blocks: BTreeMap::new(),
            definitions: BTreeMap::new(),
            uses: BTreeMap::new(),
        }
    }

    /// Convert the CFG to SSA form by processing each block.
    pub fn convert(mut self) -> SsaForm {
        // Phase 1: Collect variable definitions from the CFG
        self.collect_variable_definitions();

        // Phase 2: Place φ nodes at dominance frontiers
        self.place_phi_nodes();

        // Phase 3: Initialize SSA blocks
        self.initialize_ssa_blocks();

        // Phase 4: Rename variables and build SSA statements
        if let Some(entry_id) = self.cfg.entry_block().map(|b| b.id) {
            self.rename_block(entry_id);
        }

        SsaForm {
            cfg: self.cfg.clone(),
            dominance: self.dominance,
            blocks: self.ssa_blocks,
            definitions: self.definitions,
            uses: self.uses,
        }
    }

    /// Collect variable definitions from the CFG blocks.
    fn collect_variable_definitions(&mut self) {
        // For each block, analyze the terminator to extract variable definitions
        // This is a simplified version - full implementation would need IR statements
        for block in self.cfg.blocks() {
            self.analyze_block_for_variables(block.id);
        }
    }

    /// Analyze a block to find variable definitions.
    fn analyze_block_for_variables(&mut self, block_id: BlockId) {
        // Placeholder: In full implementation, this would scan IR statements
        // For now, we track blocks that define variables based on the terminator
        if let Some(block) = self.cfg.block(block_id) {
            match &block.terminator {
                crate::decompiler::cfg::Terminator::Return => {
                    // Return blocks don't define variables
                }
                crate::decompiler::cfg::Terminator::Jump { .. } => {
                    // Simple jumps don't define variables
                }
                _ => {
                    // Other terminators might define variables in full implementation
                }
            }
        }
    }

    /// Place φ nodes at dominance frontiers for each variable.
    fn place_phi_nodes(&mut self) {
        for (var_name, def_blocks) in &self.phi_locations.clone() {
            let mut worklist: Vec<BlockId> = def_blocks.iter().copied().collect();
            let mut placed = BTreeSet::new();

            while let Some(block) = worklist.pop() {
                for frontier_block in self.dominance.dominance_frontier_vec(block) {
                    if placed.insert(frontier_block) {
                        self.phi_locations
                            .entry(var_name.clone())
                            .or_default()
                            .insert(frontier_block);
                        worklist.push(frontier_block);
                    }
                }
            }
        }
    }

    /// Initialize empty SSA blocks for each CFG block.
    fn initialize_ssa_blocks(&mut self) {
        for block in self.cfg.blocks() {
            let ssa_block = SsaBlock::new();

            // Add φ nodes for this block
            if let Some(locations) = self.phi_locations.get("") {
                for _block_id in locations {
                    // φ nodes will be added when we have actual variables
                }
            }

            self.ssa_blocks.insert(block.id, ssa_block);
        }
    }

    /// Rename variables in a block and its dominator tree children.
    fn rename_block(&mut self, block_id: BlockId) {
        // Process φ nodes first (they define new versions)
        self.process_phi_nodes(block_id);

        // Process regular statements
        self.process_statements(block_id);

        // Recursively process children in dominator tree
        let children: Vec<BlockId> = self.dominance.children(block_id).to_vec();
        for child in children {
            self.rename_block(child);
        }

        // Pop versions for this block
        self.pop_block_versions(block_id);
    }

    /// Process φ nodes at the start of a block.
    fn process_phi_nodes(&mut self, block_id: BlockId) {
        // Collect φ nodes first to avoid mutable borrow issues
        let phi_nodes: Vec<_> = self
            .ssa_blocks
            .get(&block_id)
            .map(|block| block.phi_nodes.clone())
            .unwrap_or_default();

        for phi in &phi_nodes {
            let new_var = self.new_version(phi.target.base.clone());
            if let Some(ssa_block) = self.ssa_blocks.get_mut(&block_id) {
                ssa_block.add_stmt(SsaStmt::assign(new_var.clone(), SsaExpr::var(new_var.clone())));
            }
            self.definitions.insert(new_var, block_id);
        }
    }

    /// Process regular statements in a block.
    fn process_statements(&mut self, block_id: BlockId) {
        // Placeholder: In full implementation, this would convert IR statements
        // to SSA statements with proper variable versioning
        if let Some(block) = self.cfg.block(block_id) {
            match &block.terminator {
                crate::decompiler::cfg::Terminator::Return => {
                    // Return statement - no variable definition
                }
                crate::decompiler::cfg::Terminator::Jump { target } => {
                    // Record the jump for potential φ node operands
                    self.record_jump_edge(block_id, *target);
                }
                _ => {}
            }
        }
    }

    /// Record a jump edge for φ node operand tracking.
    fn record_jump_edge(&mut self, _source: BlockId, _target: BlockId) {
        // Placeholder: In full implementation, this would track which
        // variable versions flow into φ nodes from each predecessor
    }

    /// Pop variable versions when exiting a block.
    fn pop_block_versions(&mut self, _block_id: BlockId) {
        // Placeholder: In full implementation, this would pop versions
        // for variables defined in this block
    }

    /// Get the current SSA version of a variable.
    fn current_version(&self, base: &str) -> Option<&SsaVariable> {
        self.version_stack.get(base).and_then(|stack| stack.last())
    }

    /// Create a new version of a variable.
    fn new_version(&mut self, base: String) -> SsaVariable {
        let version = self.versions.entry(base.clone()).or_insert(0);
        let var = SsaVariable::new(base.clone(), *version);
        *version += 1;
        self.push_version(base, var.clone());
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

/// Convert an IR expression to an SSA expression.
pub fn expr_to_ssa(expr: &Expr) -> SsaExpr {
    match expr {
        Expr::Literal(lit) => SsaExpr::Literal(lit.clone()),
        Expr::Variable(name) => {
            // For now, use the variable name as-is
            // Full implementation would look up the current SSA version
            SsaExpr::var(SsaVariable::initial(name.clone()))
        }
        Expr::Binary { op, left, right } => SsaExpr::binary(
            op.clone(),
            expr_to_ssa(left),
            expr_to_ssa(right),
        ),
        Expr::Unary { op, operand } => {
            SsaExpr::unary(op.clone(), expr_to_ssa(operand))
        }
        Expr::Call { name, args } => SsaExpr::call(
            name.clone(),
            args.iter().map(expr_to_ssa).collect(),
        ),
        Expr::Index { base, index } => SsaExpr::Index {
            base: Box::new(expr_to_ssa(base)),
            index: Box::new(expr_to_ssa(index)),
        },
        Expr::Member { base, name } => SsaExpr::Member {
            base: Box::new(expr_to_ssa(base)),
            name: name.clone(),
        },
        Expr::Cast { expr, target_type } => SsaExpr::Cast {
            expr: Box::new(expr_to_ssa(expr)),
            target_type: target_type.clone(),
        },
        Expr::Array(elements) => {
            SsaExpr::Array(elements.iter().map(expr_to_ssa).collect())
        }
        Expr::Map(pairs) => SsaExpr::Map(
            pairs
                .iter()
                .map(|(k, v)| (expr_to_ssa(k), expr_to_ssa(v)))
                .collect(),
        ),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => SsaExpr::Ternary {
            condition: Box::new(expr_to_ssa(condition)),
            then_expr: Box::new(expr_to_ssa(then_expr)),
            else_expr: Box::new(expr_to_ssa(else_expr)),
        },
        Expr::StackTemp(n) => {
            // Stack temporaries become SSA variables with a special naming
            SsaExpr::var(SsaVariable::initial(format!("stack_{}", n)))
        }
    }
}

/// Convert an IR statement to an SSA statement.
///
/// Note: This is a simplified conversion that wraps IR statements in `SsaStmt::Other`.
/// Full implementation would convert all statement types to proper SSA form.
pub fn stmt_to_ssa(stmt: &Stmt) -> SsaStmt {
    match stmt {
        Stmt::Assign { target, value } => {
            let target_var = SsaVariable::initial(target.clone());
            SsaStmt::assign(target_var, expr_to_ssa(value))
        }
        // For statements that don't define SSA variables, wrap them as-is
        _ => SsaStmt::other(stmt.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompiler::ir::BinOp;

    #[test]
    fn test_expr_to_ssa_literal() {
        let expr = Expr::Literal(Literal::Int(42));
        let ssa_expr = expr_to_ssa(&expr);
        assert!(matches!(ssa_expr, SsaExpr::Literal(Literal::Int(42))));
    }

    #[test]
    fn test_expr_to_ssa_variable() {
        let expr = Expr::var("x");
        let ssa_expr = expr_to_ssa(&expr);
        assert!(matches!(ssa_expr, SsaExpr::Variable(_)));
    }

    #[test]
    fn test_expr_to_ssa_binary() {
        let expr = Expr::binary(BinOp::Add, Expr::int(1), Expr::int(2));
        let ssa_expr = expr_to_ssa(&expr);
        assert!(matches!(ssa_expr, SsaExpr::Binary { .. }));
    }

    #[test]
    fn test_stmt_to_ssa_assign() {
        let stmt = Stmt::assign("x", Expr::int(42));
        let ssa_stmt = stmt_to_ssa(&stmt);
        assert!(matches!(ssa_stmt, SsaStmt::Assign { .. }));
    }
}
