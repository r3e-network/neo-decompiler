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
    /// A resolved call that cannot return normally.
    NoReturnCall,
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
        /// Whether this leaves the owning try/catch region non-locally.
        nonlocal: bool,
    },
    /// ENDTRY that first executes its owning finally region.
    EndTryFinally {
        /// Logical block resumed after ENDFINALLY.
        continuation: BlockId,
        /// First block of the owning finally region.
        finally_target: BlockId,
        /// Whether the continuation is a non-local leave from the try arm.
        nonlocal: bool,
    },
    /// ENDFINALLY dispatches to the continuation saved by the entering ENDTRY.
    /// An exceptional entry rethrows instead of taking any normal successor.
    EndFinally {
        /// All normal continuations associated with this physical finally body.
        normal_continuations: Vec<BlockId>,
    },
    /// Unknown or unanalyzed terminator.
    Unknown,
}

impl Terminator {
    /// Get all successor block IDs.
    #[must_use]
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Terminator::Fallthrough { target } => vec![*target],
            Terminator::Jump { target } => vec![*target],
            Terminator::Branch {
                then_target,
                else_target,
            } => vec![*then_target, *else_target],
            Terminator::Return
            | Terminator::Throw
            | Terminator::Abort
            | Terminator::NoReturnCall => vec![],
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
            Terminator::EndTry { continuation, .. } => vec![*continuation],
            Terminator::EndTryFinally { finally_target, .. } => vec![*finally_target],
            Terminator::EndFinally {
                normal_continuations,
            } => normal_continuations.clone(),
            Terminator::Unknown => vec![],
        }
    }

    /// Check if this terminator can fall through to the next instruction.
    #[must_use]
    pub fn can_fallthrough(&self) -> bool {
        matches!(self, Terminator::Fallthrough { .. })
    }

    /// Check if this is a conditional branch.
    #[must_use]
    pub fn is_conditional(&self) -> bool {
        matches!(self, Terminator::Branch { .. })
    }
}
