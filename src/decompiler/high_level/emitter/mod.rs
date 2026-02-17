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

use helpers::{convert_target_name, format_int_bytes_as_decimal, format_pushdata, format_type_operand, literal_from_operand};
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
    local_pointer_values: BTreeMap<usize, usize>,
    static_pointer_values: BTreeMap<usize, usize>,
    argument_labels: BTreeMap<usize, String>,
    literal_values: BTreeMap<String, LiteralValue>,
    /// Concrete element lists for values emitted by PACK with literal count.
    /// Enables precise UNPACK stack modeling when those values are loaded later.
    packed_values_by_name: BTreeMap<String, Vec<String>>,
    /// Pre-resolved labels for CALLT method-token indices.
    /// Index `i` corresponds to method-token index `i` in the NEF file.
    callt_labels: Vec<String>,
    /// Declared parameter counts for CALLT method-token indices.
    callt_param_counts: Vec<usize>,
    /// Declared return-value flags for CALLT method-token indices.
    callt_returns_value: Vec<bool>,
    /// Stack snapshots saved before entering if-bodies so that the stack can
    /// be restored when the closing brace is emitted.  Keyed by the offset
    /// where the closer will be placed (i.e. the false-target of the branch).
    branch_saved_stacks: BTreeMap<usize, Vec<String>>,
    /// Pre-resolved method names keyed by method start offset.
    /// Used to replace `call_0xXXXX()` placeholders when a stable symbol exists.
    method_labels_by_offset: BTreeMap<usize, String>,
    /// Resolved argument counts keyed by method start offset.
    /// Used to preserve call-site argument expressions for internal calls.
    method_arg_counts_by_offset: BTreeMap<usize, usize>,
    /// Resolved internal CALL/CALL_L targets keyed by call instruction offset.
    /// Used when the immediate call target lands inside a method body.
    call_targets_by_offset: BTreeMap<usize, usize>,
    /// Resolved internal targets keyed by CALLA instruction offset.
    /// Used when pointer provenance is outside the current method body.
    calla_targets_by_offset: BTreeMap<usize, usize>,
    /// Method start offsets whose bodies always terminate without returning
    /// (every exit path ends with ABORT, ABORTMSG, or THROW).
    noreturn_method_offsets: BTreeSet<usize>,
    /// Pre-branch stack depths keyed by merge offset.  Used to detect
    /// when both branches of an if/else produce stack values that must
    /// be unified at the merge point (phi-variable reconciliation).
    pre_branch_stack_depth: BTreeMap<usize, usize>,
    /// When true, the current method is declared void in the manifest.
    /// `emit_return` will emit `return;` instead of `return <value>;`.
    returns_void: bool,
    /// Ranges `[start, end)` of finally bodies already registered.
    /// Used to suppress duplicate `finally {` when an outer TRY's finally
    /// target falls inside an inner TRY's already-registered finally body.
    finally_body_ranges: Vec<(usize, usize)>,
}

pub(crate) struct HighLevelOutput {
    pub(crate) statements: Vec<String>,
    pub(crate) warnings: Vec<String>,
}
