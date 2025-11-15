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

/// Return the complete table of known syscalls.
pub fn all() -> &'static [SyscallInfo] {
    generated::SYSCALLS
}
