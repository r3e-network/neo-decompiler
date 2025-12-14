use super::super::basic_block::BlockId;

/// An edge in the CFG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    /// Source block.
    pub from: BlockId,
    /// Target block.
    pub to: BlockId,
    /// Kind of edge.
    pub kind: EdgeKind,
}

/// The kind of CFG edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    /// Unconditional edge (fallthrough or jump).
    Unconditional,
    /// Conditional true branch.
    ConditionalTrue,
    /// Conditional false branch.
    ConditionalFalse,
    /// Exception handler edge.
    Exception,
    /// Finally block edge.
    Finally,
}
