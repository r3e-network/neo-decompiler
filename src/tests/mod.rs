//! Integration tests for the Neo N3 decompiler

pub mod syscall_integration_test;

#[cfg(test)]
mod basic_tests {
    use crate::{Decompiler, DecompilerConfig};

    #[test]
    fn test_decompiler_creation() {
        let config = DecompilerConfig::default();
        let _decompiler = Decompiler::new(config);
    }
}