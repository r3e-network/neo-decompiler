pub(super) mod expr;
pub(super) mod expr_calls;
pub(super) mod expr_context;
pub(super) mod expr_inline;
pub(super) mod expr_intrinsics;
pub(super) mod expr_low_level;
pub(super) mod expr_native;
pub(super) mod expr_syscalls;
pub(super) mod expr_values;
pub(super) mod native_framework;
pub(super) mod nullability;
pub(super) mod plan;
pub(super) mod plan_activity;
pub(super) mod stmt;

pub(crate) fn is_exact_syscall(hash: u32) -> bool {
    expr_syscalls::is_exact_syscall(hash)
}

#[cfg(test)]
mod tests;
