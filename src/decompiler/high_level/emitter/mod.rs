use std::collections::{BTreeMap, BTreeSet};

use crate::instruction::Instruction;

mod control_flow;
mod core;
mod dispatch;
mod helpers;
mod postprocess;
mod slots;
mod stack;
mod types;
mod util;

use helpers::{convert_target_name, format_pushdata, literal_from_operand};
use types::{DoWhileLoop, LiteralValue, LoopContext, LoopJump, SlotKind};

#[derive(Debug, Default)]
pub(crate) struct HighLevelEmitter {
    stack: Vec<String>,
    statements: Vec<String>,
    warnings: Vec<String>,
    next_temp: usize,
    inline_single_use_temps: bool,
    pending_closers: BTreeMap<usize, usize>,
    else_targets: BTreeMap<usize, usize>,
    pending_if_headers: BTreeMap<usize, Vec<String>>,
    catch_targets: BTreeMap<usize, usize>,
    finally_targets: BTreeMap<usize, usize>,
    skip_jumps: BTreeSet<usize>,
    transfer_labels: BTreeSet<usize>,
    program: Vec<Instruction>,
    index_by_offset: BTreeMap<usize, usize>,
    do_while_headers: BTreeMap<usize, Vec<DoWhileLoop>>,
    active_do_while_tails: BTreeSet<usize>,
    loop_stack: Vec<LoopContext>,
    initialized_locals: BTreeSet<usize>,
    initialized_statics: BTreeSet<usize>,
    argument_labels: BTreeMap<usize, String>,
    literal_values: BTreeMap<String, LiteralValue>,
    /// Pre-resolved labels for CALLT method-token indices.
    /// Index `i` corresponds to method-token index `i` in the NEF file.
    callt_labels: Vec<String>,
    /// Stack snapshots saved before entering if-bodies so that the stack can
    /// be restored when the closing brace is emitted.  Keyed by the offset
    /// where the closer will be placed (i.e. the false-target of the branch).
    branch_saved_stacks: BTreeMap<usize, Vec<String>>,
}

pub(crate) struct HighLevelOutput {
    pub(crate) statements: Vec<String>,
    pub(crate) warnings: Vec<String>,
}
