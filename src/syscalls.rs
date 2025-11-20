//! Lookup table for Neo N3 syscall metadata.

mod generated {
    include!("syscalls_generated.rs");
}

/// Metadata describing a syscall.
pub use generated::SyscallInfo;

/// Find information about a syscall by its numeric hash.
pub fn lookup(hash: u32) -> Option<&'static SyscallInfo> {
    generated::SYSCALLS.iter().find(|info| info.hash == hash)
}

/// Returns true if the syscall is known to push a value onto the stack.
pub fn returns_value(hash: u32) -> bool {
    lookup(hash).map(|info| info.returns_value).unwrap_or(true)
}

/// Return the complete table of known syscalls.
pub fn all() -> &'static [SyscallInfo] {
    generated::SYSCALLS
}

/// Return a human-readable summary of a syscall suitable for catalogs.
pub fn summarize(syscall: &SyscallInfo) -> (&'static str, &'static str, bool) {
    (syscall.name, syscall.call_flags, syscall.returns_value)
}
