use super::block_id::BlockId;

/// How a basic block terminates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminator {
    /// Falls through to the next block.
    Fallthrough {
        /// The block to fall through to.
        target: BlockId,
    },
    /// Unconditional jump to a target block.
    Jump {
        /// The block to jump to.
        target: BlockId,
    },
    /// Conditional branch: if true go to then_target, else fall through to else_target.
    Branch {
        /// Block executed when condition is true.
        then_target: BlockId,
        /// Block executed when condition is false.
        else_target: BlockId,
    },
    /// Return from the function.
    Return,
    /// Throw an exception.
    Throw,
    /// Abort execution.
    Abort,
    /// Try block entry with catch/finally targets.
    TryEntry {
        /// The main try body block.
        body_target: BlockId,
        /// Optional catch handler block.
        catch_target: Option<BlockId>,
        /// Optional finally handler block.
        finally_target: Option<BlockId>,
    },
    /// End of try/catch/finally block.
    EndTry {
        /// Block to continue execution after try/catch/finally.
        continuation: BlockId,
    },
    /// Unknown or unanalyzed terminator.
    Unknown,
}

impl Terminator {
    /// Get all successor block IDs.
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Terminator::Fallthrough { target } => vec![*target],
            Terminator::Jump { target } => vec![*target],
            Terminator::Branch {
                then_target,
                else_target,
            } => vec![*then_target, *else_target],
            Terminator::Return | Terminator::Throw | Terminator::Abort => vec![],
            Terminator::TryEntry {
                body_target,
                catch_target,
                finally_target,
            } => {
                let mut succs = vec![*body_target];
                if let Some(c) = catch_target {
                    succs.push(*c);
                }
                if let Some(f) = finally_target {
                    succs.push(*f);
                }
                succs
            }
            Terminator::EndTry { continuation } => vec![*continuation],
            Terminator::Unknown => vec![],
        }
    }

    /// Check if this terminator can fall through to the next instruction.
    pub fn can_fallthrough(&self) -> bool {
        matches!(self, Terminator::Fallthrough { .. })
    }

    /// Check if this is a conditional branch.
    pub fn is_conditional(&self) -> bool {
        matches!(self, Terminator::Branch { .. })
    }
}
