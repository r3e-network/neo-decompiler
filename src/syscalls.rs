//! Lookup table for Neo N3 syscall metadata.

#[allow(missing_docs)]
mod generated {
    include!("syscalls_generated.rs");
}

/// Metadata describing a syscall.
pub use generated::SyscallInfo;

#[allow(dead_code)]
const fn assert_syscalls_sorted_by_hash(syscalls: &[SyscallInfo]) {
    let mut i = 1usize;
    while i < syscalls.len() {
        if syscalls[i - 1].hash >= syscalls[i].hash {
            panic!("generated::SYSCALLS must be sorted by hash (strictly increasing)");
        }
        i += 1;
    }
}

const _: () = assert_syscalls_sorted_by_hash(generated::SYSCALLS);

/// Find information about a syscall by its numeric hash.
pub fn lookup(hash: u32) -> Option<&'static SyscallInfo> {
    generated::SYSCALLS
        .binary_search_by_key(&hash, |info| info.hash)
        .ok()
        .map(|index| &generated::SYSCALLS[index])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_finds_every_syscall() {
        for info in all() {
            let got = lookup(info.hash).expect("expected syscall to be present");
            assert_eq!(got.hash, info.hash);
            assert_eq!(got.name, info.name);
            assert_eq!(got.handler, info.handler);
            assert_eq!(got.price, info.price);
            assert_eq!(got.call_flags, info.call_flags);
            assert_eq!(got.returns_value, info.returns_value);
        }
    }

    #[test]
    fn lookup_unknown_hash_returns_none() {
        assert!(lookup(0).is_none());
        assert!(lookup(u32::MAX).is_none());
    }

    #[test]
    fn returns_value_matches_table_for_known_syscalls() {
        for info in all() {
            assert_eq!(returns_value(info.hash), info.returns_value);
        }
    }

    #[test]
    fn returns_value_defaults_to_true_for_unknown_syscalls() {
        assert!(returns_value(0));
        assert!(returns_value(u32::MAX));
    }

    #[test]
    fn syscall_table_is_sorted_by_hash() {
        let syscalls = all();
        for window in syscalls.windows(2) {
            assert!(window[0].hash < window[1].hash);
        }
    }

    #[test]
    fn summarize_matches_syscall_fields() {
        let info = &all()[0];
        assert_eq!(
            summarize(info),
            (info.name, info.call_flags, info.returns_value)
        );
    }
}
