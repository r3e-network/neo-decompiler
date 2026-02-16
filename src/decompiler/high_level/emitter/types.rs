#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SlotKind {
    Local,
    Argument,
    Static,
}

#[derive(Clone, Debug)]
pub(super) struct LoopContext {
    pub(super) break_offset: usize,
    pub(super) continue_offset: usize,
}

#[derive(Clone, Debug)]
pub(super) struct DoWhileLoop {
    pub(super) tail_offset: usize,
    pub(super) break_offset: usize,
}

#[derive(Clone, Debug)]
pub(super) struct LoopJump {
    pub(super) jump_offset: usize,
    pub(super) target: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum LiteralValue {
    Integer(i64),
    Pointer(usize),
    Boolean(bool),
    String(String),
}
