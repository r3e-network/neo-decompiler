/// Unique identifier for a basic block within a CFG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub usize);

impl BlockId {
    /// The entry block ID (always 0).
    pub const ENTRY: BlockId = BlockId(0);

    /// Create a new block ID.
    pub fn new(id: usize) -> Self {
        BlockId(id)
    }

    /// Get the numeric ID.
    #[must_use]
    pub fn index(self) -> usize {
        self.0
    }
}

impl From<usize> for BlockId {
    fn from(value: usize) -> Self {
        BlockId(value)
    }
}

impl std::fmt::Display for BlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BB{}", self.0)
    }
}
