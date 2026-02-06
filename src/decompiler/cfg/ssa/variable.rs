//! SSA variable and φ node types.
//!
//! Static Single Assignment (SSA) form ensures each variable is assigned exactly once.
//! We achieve this by versioning variables: each definition creates a new (name, version) pair.

use std::fmt;

use crate::decompiler::cfg::BlockId;

/// A versioned variable in SSA form.
///
/// Each SSA variable combines a base name (like `"local_0"` or `"arg_1"`) with a
/// version number. The SSA property guarantees that each unique `(base, version)`
/// pair is assigned exactly once.
///
/// # Display
///
/// `SsaVariable` implements `Display` to show only the base name, hiding the
/// version from end users. This keeps SSA as an internal analysis detail while
/// maintaining clean output.
///
/// # Examples
///
/// ```
/// use neo_decompiler::decompiler::cfg::ssa::SsaVariable;
///
/// let v0 = SsaVariable::initial("x".to_string());
/// let v1 = v0.next();
///
/// assert_eq!(v0.base, "x");
/// assert_eq!(v0.version, 0);
/// assert_eq!(v1.version, 1);
///
/// // Display hides the version
/// assert_eq!(v0.to_string(), "x");
/// assert_eq!(v1.to_string(), "x");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SsaVariable {
    /// Base variable name (e.g., "local_0", "arg_1", "static_2").
    pub base: String,
    /// Version number ensuring single-assignment property.
    pub version: usize,
}

impl SsaVariable {
    /// Create a new SSA variable with a specific version.
    #[must_use]
    pub const fn new(base: String, version: usize) -> Self {
        Self { base, version }
    }

    /// Create an initial SSA variable (version 0).
    ///
    /// Use this for the first definition of a variable.
    #[must_use]
    pub fn initial(base: String) -> Self {
        Self::new(base, 0)
    }

    /// Create the next version of this variable.
    ///
    /// Used during SSA renaming when a new definition is encountered.
    #[must_use]
    pub fn next(&self) -> Self {
        Self::new(self.base.clone(), self.version + 1)
    }

    /// Check if this is the initial version (version 0).
    #[must_use]
    pub const fn is_initial(&self) -> bool {
        self.version == 0
    }
}

impl fmt::Display for SsaVariable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display only the base name, hiding version from users
        write!(f, "{}", self.base)
    }
}

/// A φ (phi) node representing a merge point in the CFG.
///
/// φ nodes are inserted at control flow merge points (dominance frontiers) to
/// ensure the single-assignment property. Each φ node selects a value based on
/// which predecessor block was executed.
///
/// # Structure
///
/// ```text
///     block1    block2
///       |          |
///       v          v
///        \        /
///         merge_block
///         x3 = φ(x1, x2)
/// ```
///
/// At runtime, `x3` takes the value of `x1` if control came from `block1`,
/// or `x2` if it came from `block2`.
///
/// # Internal Use Only
///
/// φ nodes are used internally for analysis and are typically transformed away
/// before rendering output. Users should not see raw φ nodes in the decompiled
/// code.
///
/// # Examples
///
/// ```
/// use neo_decompiler::decompiler::cfg::ssa::{PhiNode, SsaVariable};
/// use neo_decompiler::decompiler::cfg::BlockId;
///
/// let target = SsaVariable::initial("result".to_string());
/// let mut phi = PhiNode::new(target.clone());
///
/// let block1 = BlockId::from(1);
/// let block2 = BlockId::from(2);
///
/// phi.add_operand(block1, SsaVariable::new("x".to_string(), 0));
/// phi.add_operand(block2, SsaVariable::new("x".to_string(), 1));
///
/// assert_eq!(phi.operands.len(), 2);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct PhiNode {
    /// The target SSA variable being defined by this φ node.
    pub target: SsaVariable,
    /// Operands from each predecessor block.
    ///
    /// Maps predecessor block IDs to the SSA variable that flows from that edge.
    pub operands: std::collections::BTreeMap<BlockId, SsaVariable>,
}

impl PhiNode {
    /// Create a new φ node with an empty operand list.
    #[must_use]
    pub const fn new(target: SsaVariable) -> Self {
        Self {
            target,
            operands: std::collections::BTreeMap::new(),
        }
    }

    /// Add an operand from a predecessor block.
    ///
    /// # Arguments
    ///
    /// * `predecessor` - The block ID of the predecessor.
    /// * `var` - The SSA variable that reaches this φ node from that predecessor.
    pub fn add_operand(&mut self, predecessor: BlockId, var: SsaVariable) {
        self.operands.insert(predecessor, var);
    }

    /// Get the number of operands (predecessors).
    #[must_use]
    pub fn operand_count(&self) -> usize {
        self.operands.len()
    }
}

impl fmt::Display for PhiNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} = φ(", self.target)?;
        let mut first = true;
        for (block, var) in &self.operands {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "{:?}: {}", block, var)?;
            first = false;
        }
        write!(f, ")")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssa_variable_versioning() {
        let v0 = SsaVariable::initial("x".to_string());
        assert_eq!(v0.base, "x");
        assert_eq!(v0.version, 0);
        assert!(v0.is_initial());

        let v1 = v0.next();
        assert_eq!(v1.base, "x");
        assert_eq!(v1.version, 1);
        assert!(!v1.is_initial());

        let v2 = v1.next();
        assert_eq!(v2.version, 2);
    }

    #[test]
    fn test_ssa_variable_display_hides_version() {
        let v0 = SsaVariable::initial("local_0".to_string());
        let v1 = v0.next();
        let v2 = v1.next();

        // All display as the base name
        assert_eq!(v0.to_string(), "local_0");
        assert_eq!(v1.to_string(), "local_0");
        assert_eq!(v2.to_string(), "local_0");
    }

    #[test]
    fn test_ssa_variable_ord() {
        // SSA variables must be ordered for BTreeMap
        let mut set = std::collections::BTreeSet::new();
        set.insert(SsaVariable::new("z".to_string(), 1));
        set.insert(SsaVariable::new("a".to_string(), 2));
        set.insert(SsaVariable::new("m".to_string(), 0));

        let vars: Vec<_> = set.iter().collect();
        assert_eq!(vars[0].base, "a");
        assert_eq!(vars[1].base, "m");
        assert_eq!(vars[2].base, "z");
    }

    #[test]
    fn test_phi_node_creation() {
        let target = SsaVariable::initial("result".to_string());
        let phi = PhiNode::new(target);

        assert_eq!(phi.target.base, "result");
        assert!(phi.operands.is_empty());
        assert_eq!(phi.operand_count(), 0);
    }

    #[test]
    fn test_phi_node_add_operands() {
        let target = SsaVariable::initial("x".to_string());
        let mut phi = PhiNode::new(target);

        let block1 = BlockId(1);
        let block2 = BlockId(2);

        phi.add_operand(block1, SsaVariable::new("y".to_string(), 0));
        phi.add_operand(block2, SsaVariable::new("z".to_string(), 0));

        assert_eq!(phi.operand_count(), 2);
        assert!(phi.operands.contains_key(&block1));
        assert!(phi.operands.contains_key(&block2));
    }

    #[test]
    fn test_phi_node_display() {
        let target = SsaVariable::initial("result".to_string());
        let mut phi = PhiNode::new(target);

        let block1 = BlockId(0);
        let block2 = BlockId(1);

        phi.add_operand(block1, SsaVariable::new("x".to_string(), 0));
        phi.add_operand(block2, SsaVariable::new("x".to_string(), 1));

        let display = phi.to_string();
        assert!(display.contains("result = φ("));
    }
}
