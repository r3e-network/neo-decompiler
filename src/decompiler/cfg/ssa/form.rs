//! SSA form types for representing code in static single assignment form.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::decompiler::cfg::{BlockId, Cfg};
use crate::decompiler::ir::{BinOp, Literal, Stmt, UnaryOp};

use super::dominance::DominanceInfo;
use super::variable::{PhiNode, SsaVariable};

/// A control flow graph in Static Single Assignment form.
///
/// SSA form guarantees that each variable is assigned exactly once, making
/// data flow analysis and optimizations significantly simpler.
///
/// # Structure
///
/// - `cfg`: The original control flow graph
/// - `dominance`: Pre-computed dominance relationships
/// - `blocks`: Each basic block with φ nodes at the start, followed by SSA statements
/// - `definitions`: Mapping from SSA variables to their defining blocks
/// - `uses`: Mapping from SSA variables to their use sites
///
/// # Examples
///
/// ```
/// use neo_decompiler::decompiler::cfg::Cfg;
///
/// let cfg = /* ... */;
/// let ssa = cfg.to_ssa();
///
/// // Query variable definitions
/// for (var, block) in &ssa.definitions {
///     println!("{:?} defined in block {:?}", var, block);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct SsaForm {
    /// The original control flow graph.
    pub cfg: Cfg,

    /// Dominance information (immediate dominators, dominator tree, dominance frontiers).
    pub dominance: DominanceInfo,

    /// SSA blocks indexed by block ID.
    pub blocks: BTreeMap<BlockId, SsaBlock>,

    /// Mapping from SSA variables to the block where they are defined.
    pub definitions: BTreeMap<SsaVariable, BlockId>,

    /// Mapping from SSA variables to all their use sites.
    pub uses: BTreeMap<SsaVariable, BTreeSet<UseSite>>,
}

impl SsaForm {
    /// Create a new empty SSA form.
    #[must_use]
    pub fn new(cfg: Cfg, dominance: DominanceInfo) -> Self {
        Self {
            cfg,
            dominance,
            blocks: BTreeMap::new(),
            definitions: BTreeMap::new(),
            uses: BTreeMap::new(),
        }
    }

    /// Add a block to the SSA form.
    pub fn add_block(&mut self, id: BlockId, block: SsaBlock) {
        self.blocks.insert(id, block);
    }

    /// Get a block by ID.
    #[must_use]
    pub fn block(&self, id: BlockId) -> Option<&SsaBlock> {
        self.blocks.get(&id)
    }

    /// Get all blocks in SSA form.
    #[must_use]
    pub fn blocks_iter(&self) -> impl Iterator<Item = (&BlockId, &SsaBlock)> {
        self.blocks.iter()
    }

    /// Get the number of blocks.
    #[must_use]
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Record a variable definition.
    pub fn add_definition(&mut self, var: SsaVariable, block: BlockId) {
        self.definitions.insert(var, block);
    }

    /// Record a variable use.
    pub fn add_use(&mut self, var: SsaVariable, site: UseSite) {
        self.uses.entry(var).or_default().insert(site);
    }

    /// Get all use sites for a variable.
    #[must_use]
    pub fn uses_of(&self, var: &SsaVariable) -> Option<&BTreeSet<UseSite>> {
        self.uses.get(var)
    }

    /// Render the SSA form as a string for debugging/display.
    ///
    /// This produces a human-readable representation of the SSA code,
    /// with φ nodes shown at the start of each block (marked with φ).
    /// φ nodes are internal analysis constructs and may be transformed
    /// away before final output.
    #[must_use]
    pub fn render(&self) -> String {
        let mut output = String::new();
        use std::fmt::Write;

        writeln!(output, "// SSA Form - {} blocks", self.block_count()).unwrap();
        writeln!(output).unwrap();

        for (block_id, block) in self.blocks_iter() {
            writeln!(output, "block {:?}:", block_id).unwrap();
            write!(output, "{}", block).unwrap();
        }

        output
    }

    /// Get statistics about the SSA form.
    #[must_use]
    pub fn stats(&self) -> SsaStats {
        let total_phi_nodes: usize = self.blocks.values().map(|b| b.phi_count()).sum();
        let total_statements: usize = self.blocks.values().map(|b| b.stmt_count()).sum();
        let total_variables = self.definitions.len();

        SsaStats {
            block_count: self.block_count(),
            total_phi_nodes,
            total_statements,
            total_variables,
        }
    }
}

/// Statistics about an SSA form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsaStats {
    /// Number of blocks in the SSA form.
    pub block_count: usize,
    /// Total number of φ nodes across all blocks.
    pub total_phi_nodes: usize,
    /// Total number of SSA statements (excluding φ nodes).
    pub total_statements: usize,
    /// Total number of unique SSA variables.
    pub total_variables: usize,
}

impl fmt::Display for SsaStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SSA Stats: {} blocks, {} φ nodes, {} statements, {} variables",
            self.block_count,
            self.total_phi_nodes,
            self.total_statements,
            self.total_variables
        )
    }
}

/// A basic block in SSA form.
///
/// SSA blocks have φ nodes at the beginning (before any regular statements),
/// followed by the SSA-converted statements.
#[derive(Debug, Clone, Default)]
pub struct SsaBlock {
    /// φ nodes at the start of this block.
    ///
    /// These must come first, as they conceptually execute at the edge
    /// from each predecessor.
    pub phi_nodes: Vec<PhiNode>,

    /// Regular statements in SSA form.
    pub stmts: Vec<SsaStmt>,
}

impl SsaBlock {
    /// Create a new empty SSA block.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a φ node to this block.
    pub fn add_phi(&mut self, phi: PhiNode) {
        self.phi_nodes.push(phi);
    }

    /// Add a statement to this block.
    pub fn add_stmt(&mut self, stmt: SsaStmt) {
        self.stmts.push(stmt);
    }

    /// Check if this block is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.phi_nodes.is_empty() && self.stmts.is_empty()
    }

    /// Get the total number of φ nodes.
    #[must_use]
    pub fn phi_count(&self) -> usize {
        self.phi_nodes.len()
    }

    /// Get the total number of statements.
    #[must_use]
    pub fn stmt_count(&self) -> usize {
        self.stmts.len()
    }
}

impl fmt::Display for SsaBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Print φ nodes first (they conceptually execute at the edge)
        for phi in &self.phi_nodes {
            writeln!(f, "    {}", phi)?;
        }

        // Then print regular statements
        for stmt in &self.stmts {
            writeln!(f, "    {}", stmt)?;
        }

        Ok(())
    }
}

/// A statement in SSA form.
#[derive(Debug, Clone, PartialEq)]
pub enum SsaStmt {
    /// Variable assignment with SSA target.
    Assign {
        /// The SSA variable being defined.
        target: SsaVariable,
        /// The value being assigned (in SSA expression form).
        value: SsaExpr,
    },

    /// φ node (internal representation, typically transformed before output).
    Phi(PhiNode),

    /// Other statements that don't define SSA variables.
    Other(Stmt),
}

impl SsaStmt {
    /// Create an assignment statement.
    #[must_use]
    pub fn assign(target: SsaVariable, value: SsaExpr) -> Self {
        Self::Assign { target, value }
    }

    /// Create a φ node statement.
    #[must_use]
    pub const fn phi(phi: PhiNode) -> Self {
        Self::Phi(phi)
    }

    /// Wrap a regular statement.
    #[must_use]
    pub const fn other(stmt: Stmt) -> Self {
        Self::Other(stmt)
    }
}

/// An expression in SSA form.
///
/// SSA expressions reference `SsaVariable` instead of raw strings,
/// ensuring version tracking through the SSA transformation.
#[derive(Debug, Clone, PartialEq)]
pub enum SsaExpr {
    /// SSA variable reference.
    Variable(SsaVariable),

    /// Literal constant value.
    Literal(Literal),

    /// Binary operation.
    Binary {
        op: BinOp,
        left: Box<SsaExpr>,
        right: Box<SsaExpr>,
    },

    /// Unary operation.
    Unary {
        op: UnaryOp,
        operand: Box<SsaExpr>,
    },

    /// Function or syscall invocation.
    Call {
        name: String,
        args: Vec<SsaExpr>,
    },

    /// Array/map index access.
    Index {
        base: Box<SsaExpr>,
        index: Box<SsaExpr>,
    },

    /// Field/member access.
    Member {
        base: Box<SsaExpr>,
        name: String,
    },

    /// Type cast.
    Cast {
        expr: Box<SsaExpr>,
        target_type: String,
    },

    /// Array literal.
    Array(Vec<SsaExpr>),

    /// Map literal (key-value pairs).
    Map(Vec<(SsaExpr, SsaExpr)>),

    /// Ternary conditional expression.
    Ternary {
        condition: Box<SsaExpr>,
        then_expr: Box<SsaExpr>,
        else_expr: Box<SsaExpr>,
    },
}

impl SsaExpr {
    /// Create a variable reference.
    #[must_use]
    pub fn var(var: SsaVariable) -> Self {
        Self::Variable(var)
    }

    /// Create a literal expression.
    #[must_use]
    pub const fn lit(literal: Literal) -> Self {
        Self::Literal(literal)
    }

    /// Create a binary expression.
    #[must_use]
    pub fn binary(op: BinOp, left: SsaExpr, right: SsaExpr) -> Self {
        Self::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Create a unary expression.
    #[must_use]
    pub fn unary(op: UnaryOp, operand: SsaExpr) -> Self {
        Self::Unary {
            op,
            operand: Box::new(operand),
        }
    }

    /// Create a function call expression.
    #[must_use]
    pub fn call(name: String, args: Vec<SsaExpr>) -> Self {
        Self::Call { name, args }
    }
}

impl fmt::Display for SsaExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use std::fmt::Write;
        match self {
            Self::Variable(var) => write!(f, "{}", var),
            Self::Literal(lit) => write!(f, "{}", lit),
            Self::Binary { op, left, right } => write!(f, "({} {} {})", left, op, right),
            Self::Unary { op, operand } => write!(f, "{}({})", op, operand),
            Self::Call { name, args } => {
                write!(f, "{}(", name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
            Self::Index { base, index } => write!(f, "{}[{}]", base, index),
            Self::Member { base, name } => write!(f, "{}.{}", base, name),
            Self::Cast { expr, target_type } => write!(f, "{} as {}", expr, target_type),
            Self::Array(elements) => {
                write!(f, "[")?;
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", elem)?;
                }
                write!(f, "]")
            }
            Self::Map(pairs) => {
                write!(f, "{{")?;
                for (i, (key, value)) in pairs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", key, value)?;
                }
                write!(f, "}}")
            }
            Self::Ternary {
                condition,
                then_expr,
                else_expr,
            } => write!(f, "{} ? {} : {}", condition, then_expr, else_expr),
        }
    }
}

impl fmt::Display for SsaStmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Assign { target, value } => write!(f, "{} = {};", target, value),
            Self::Phi(phi) => write!(f, "{}", phi), // φ nodes have their own Display
            Self::Other(stmt) => write!(f, "{:?}", stmt), // Use debug for other statements
        }
    }
}

/// A location where a variable is used.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UseSite {
    /// The block containing the use.
    pub block: BlockId,
    /// Index of the statement within the block.
    pub stmt_index: usize,
}

impl UseSite {
    /// Create a new use site.
    #[must_use]
    pub const fn new(block: BlockId, stmt_index: usize) -> Self {
        Self { block, stmt_index }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssa_form_creation() {
        let cfg = Cfg::new();
        let dominance = DominanceInfo::new();
        let ssa = SsaForm::new(cfg, dominance);

        assert_eq!(ssa.block_count(), 0);
        assert!(ssa.definitions.is_empty());
        assert!(ssa.uses.is_empty());
    }

    #[test]
    fn test_ssa_block_additions() {
        let mut block = SsaBlock::new();

        let phi = PhiNode::new(SsaVariable::initial("x".to_string()));
        block.add_phi(phi);

        let stmt = SsaStmt::assign(
            SsaVariable::new("y".to_string(), 0),
            SsaExpr::lit(Literal::Int(42)),
        );
        block.add_stmt(stmt);

        assert_eq!(block.phi_count(), 1);
        assert_eq!(block.stmt_count(), 1);
        assert!(!block.is_empty());
    }

    #[test]
    fn test_dominance_info_empty() {
        let info = DominanceInfo::new();

        assert!(info.idom(BlockId(0)).is_none());
        assert!(info.children(BlockId(0)).is_empty());
        assert!(info.dominance_frontier_vec(BlockId(0)).is_empty());
    }

    #[test]
    fn test_ssa_expr_constructors() {
        let var = SsaVariable::initial("x".to_string());
        let expr = SsaExpr::var(var.clone());

        assert!(matches!(expr, SsaExpr::Variable(_)));

        let lit = SsaExpr::lit(Literal::Int(42));
        assert!(matches!(lit, SsaExpr::Literal(_)));

        let binary = SsaExpr::binary(
            BinOp::Add,
            SsaExpr::lit(Literal::Int(1)),
            SsaExpr::lit(Literal::Int(2)),
        );
        assert!(matches!(binary, SsaExpr::Binary { .. }));

        let call = SsaExpr::call("foo".to_string(), vec![]);
        assert!(matches!(call, SsaExpr::Call { .. }));
    }

    #[test]
    fn test_use_site() {
        let site = UseSite::new(BlockId(5), 10);

        assert_eq!(site.block, BlockId(5));
        assert_eq!(site.stmt_index, 10);
    }

    #[test]
    fn test_ssa_expr_display() {
        let var = SsaVariable::initial("x".to_string());
        assert_eq!(format!("{}", SsaExpr::var(var.clone())), "x");

        let lit = SsaExpr::lit(Literal::Int(42));
        assert_eq!(format!("{}", lit), "42");

        let binary = SsaExpr::binary(
            BinOp::Add,
            SsaExpr::var(var.clone()),
            SsaExpr::lit(Literal::Int(10)),
        );
        assert_eq!(format!("{}", binary), "(x + 10)");

        let unary = SsaExpr::unary(UnaryOp::Neg, SsaExpr::var(var));
        assert_eq!(format!("{}", unary), "-(x)");

        let var2 = SsaVariable::initial("x".to_string());
        let call = SsaExpr::call("foo".to_string(), vec![SsaExpr::var(var2)]);
        assert_eq!(format!("{}", call), "foo(x)");
    }

    #[test]
    fn test_ssa_stmt_display() {
        let var = SsaVariable::new("result".to_string(), 0);
        let stmt = SsaStmt::assign(var.clone(), SsaExpr::lit(Literal::Int(42)));

        assert_eq!(format!("{}", stmt), "result = 42;");
    }

    #[test]
    fn test_ssa_block_display() {
        let mut block = SsaBlock::new();

        let phi = PhiNode::new(SsaVariable::initial("x".to_string()));
        block.add_phi(phi);

        let stmt = SsaStmt::assign(
            SsaVariable::new("y".to_string(), 0),
            SsaExpr::lit(Literal::Int(42)),
        );
        block.add_stmt(stmt);

        let display = format!("{}", block);
        assert!(display.contains("φ")); // φ node should be present
        assert!(display.contains("y = 42;")); // statement should be present
    }

    #[test]
    fn test_ssa_form_render() {
        let cfg = Cfg::new();
        let dominance = DominanceInfo::new();
        let mut ssa = SsaForm::new(cfg, dominance);

        let mut block = SsaBlock::new();
        block.add_stmt(SsaStmt::assign(
            SsaVariable::new("x".to_string(), 0),
            SsaExpr::lit(Literal::Int(42)),
        ));
        ssa.add_block(BlockId::ENTRY, block);

        let rendered = ssa.render();
        assert!(rendered.contains("SSA Form"));
        assert!(rendered.contains("block"));
        assert!(rendered.contains("x = 42;"));
    }

    #[test]
    fn test_ssa_stats() {
        let cfg = Cfg::new();
        let dominance = DominanceInfo::new();
        let mut ssa = SsaForm::new(cfg, dominance);

        let mut block = SsaBlock::new();

        // Add a φ node
        let phi = PhiNode::new(SsaVariable::initial("x".to_string()));
        block.add_phi(phi);

        // Add a statement
        let stmt = SsaStmt::assign(
            SsaVariable::new("y".to_string(), 0),
            SsaExpr::lit(Literal::Int(42)),
        );
        block.add_stmt(stmt);

        ssa.add_block(BlockId::ENTRY, block);
        ssa.add_definition(SsaVariable::initial("y".to_string()), BlockId::ENTRY);

        let stats = ssa.stats();
        assert_eq!(stats.block_count, 1);
        assert_eq!(stats.total_phi_nodes, 1);
        assert_eq!(stats.total_statements, 1);
        assert_eq!(stats.total_variables, 1);
    }

    #[test]
    fn test_ssa_expr_complex() {
        // Test array literal
        let arr = SsaExpr::Array(vec![
            SsaExpr::lit(Literal::Int(1)),
            SsaExpr::lit(Literal::Int(2)),
        ]);
        assert_eq!(format!("{}", arr), "[1, 2]");

        // Test map literal
        let map = SsaExpr::Map(vec![
            (SsaExpr::lit(Literal::String("key".to_string())), SsaExpr::lit(Literal::Int(1))),
        ]);
        assert!(format!("{}", map).contains("{"));

        // Test ternary
        let ternary = SsaExpr::Ternary {
            condition: Box::new(SsaExpr::lit(Literal::Bool(true))),
            then_expr: Box::new(SsaExpr::lit(Literal::Int(1))),
            else_expr: Box::new(SsaExpr::lit(Literal::Int(2))),
        };
        assert_eq!(format!("{}", ternary), "true ? 1 : 2");
    }
}
