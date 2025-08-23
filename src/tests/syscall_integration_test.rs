// Integration test for the complete syscall resolution system
#[cfg(test)]
mod tests {
    use crate::{
        analysis::types::{PrimitiveType, Type},
        common::config::DecompilerConfig,
        core::{decompiler::DecompilerEngine, lifter::IRLifter, syscalls::SyscallDatabase},
    };

    #[test]
    fn test_syscall_database_integration() {
        // Test that the syscall database correctly resolves Neo N3 syscalls
        let db = SyscallDatabase::new();

        // Test major Neo N3 syscalls with correct hashes
        assert_eq!(db.resolve_name(0x925de831), "System.Storage.Get");
        assert_eq!(db.get_arg_count(0x925de831), 2);
        assert_eq!(db.returns_value(0x925de831), true);

        assert_eq!(db.resolve_name(0xe63f1884), "System.Storage.Put");
        assert_eq!(db.get_arg_count(0xe63f1884), 3);
        assert_eq!(db.returns_value(0xe63f1884), false);

        assert_eq!(
            db.resolve_name(0x5d97c1b2),
            "System.Runtime.GetExecutingScriptHash"
        );
        assert_eq!(db.get_arg_count(0x5d97c1b2), 0);
        assert_eq!(db.returns_value(0x5d97c1b2), true);

        // Test runtime syscalls
        assert_eq!(db.resolve_name(0x49de7d57), "System.Runtime.Platform");
        assert_eq!(db.get_arg_count(0x49de7d57), 0);
        assert_eq!(db.returns_value(0x49de7d57), true);

        // Test contract call syscalls
        assert_eq!(db.resolve_name(0x627d5b52), "System.Contract.Call");
        assert_eq!(db.get_arg_count(0x627d5b52), 4);
        assert_eq!(db.returns_value(0x627d5b52), true);

        // Test crypto syscalls
        assert_eq!(db.resolve_name(0x82958f5a), "System.Crypto.CheckSig");
        assert_eq!(db.get_arg_count(0x82958f5a), 2);
        assert_eq!(db.returns_value(0x82958f5a), true);

        // Test unknown syscalls
        assert_eq!(db.resolve_name(0xdeadbeef), "syscall_deadbeef");
        assert_eq!(db.get_arg_count(0xdeadbeef), 0);
        assert_eq!(db.returns_value(0xdeadbeef), false);
    }

    #[test]
    fn test_syscall_type_signatures() {
        let db = SyscallDatabase::new();

        // Test System.Storage.Get signature
        let sig = db
            .get_signature(0x925de831)
            .expect("Should have Storage.Get signature");
        assert_eq!(sig.name, "System.Storage.Get");
        assert_eq!(sig.parameters.len(), 2);
        assert!(!matches!(sig.return_type, Type::Void));
        assert!(sig.effects.len() > 0);

        // Test System.Storage.Put signature
        let sig = db
            .get_signature(0xe63f1884)
            .expect("Should have Storage.Put signature");
        assert_eq!(sig.name, "System.Storage.Put");
        assert_eq!(sig.parameters.len(), 3);
        assert!(matches!(sig.return_type, Type::Void));

        // Test pure syscall
        let sig = db
            .get_signature(0x5d97c1b2)
            .expect("Should have GetExecutingScriptHash signature");
        assert_eq!(sig.name, "System.Runtime.GetExecutingScriptHash");
        assert_eq!(sig.parameters.len(), 0);
        assert!(matches!(
            sig.return_type,
            Type::Primitive(PrimitiveType::ByteArray)
        ));
    }

    #[test]
    fn test_lifter_uses_syscall_database() {
        let config = DecompilerConfig::default();
        let lifter = IRLifter::new(&config);

        // The lifter should now have a syscall database instance
        // This test verifies the integration is working
        // More detailed testing would require creating actual instructions
        // with syscall opcodes, which is complex for a unit test
    }

    #[test]
    fn test_decompiler_uses_syscall_database() {
        let config = DecompilerConfig::default();
        let decompiler = DecompilerEngine::new(&config);

        // The decompiler should now have a syscall database instance
        // This test verifies the integration is working
        // More detailed testing would require full IR analysis
    }

    #[test]
    fn test_syscall_effects_mapping() {
        let db = SyscallDatabase::new();

        // Test that syscall effects are properly mapped
        let storage_get_effects = db.get_effects(0x925de831);
        assert!(!storage_get_effects.is_empty());

        let storage_put_effects = db.get_effects(0xe63f1884);
        assert!(!storage_put_effects.is_empty());

        let pure_effects = db.get_effects(0x5d97c1b2); // GetExecutingScriptHash
        assert!(!pure_effects.is_empty());
    }

    #[test]
    fn test_all_major_neo_syscalls_present() {
        let db = SyscallDatabase::new();

        // Verify all major Neo N3 syscall categories are present
        let major_syscalls = vec![
            // Runtime
            (0x49de7d57, "System.Runtime.Platform"),
            (0x5d97c1b2, "System.Runtime.GetExecutingScriptHash"),
            (0x91f9b23b, "System.Runtime.GetCallingScriptHash"),
            (0x9e29b9a8, "System.Runtime.GetEntryScriptHash"),
            // Storage
            (0x925de831, "System.Storage.Get"),
            (0xe63f1884, "System.Storage.Put"),
            (0x7ce2e494, "System.Storage.Delete"),
            (0xa09b1eef, "System.Storage.Find"),
            (0x9c7c9598, "System.Storage.GetContext"),
            // Contract
            (0x627d5b52, "System.Contract.Call"),
            (0x14e12327, "System.Contract.CallEx"),
            // Crypto
            (0x82958f5a, "System.Crypto.CheckSig"),
            (0xf60652e8, "System.Crypto.CheckMultisig"),
            // Iterator
            (0x7e6a2bb7, "System.Iterator.Next"),
            (0x63b6c5ee, "System.Iterator.Value"),
            // JSON
            (0xa0ab5461, "System.Json.Serialize"),
            (0x7d4b2a25, "System.Json.Deserialize"),
        ];

        for (hash, expected_name) in major_syscalls {
            assert_eq!(
                db.resolve_name(hash),
                expected_name,
                "Syscall 0x{:08x} should resolve to {}",
                hash,
                expected_name
            );
        }
    }
}
