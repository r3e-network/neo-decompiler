// Syscall signature database and resolution system
use std::collections::HashMap;

use crate::{
    common::{config::SyscallDefinition, errors::ConfigError},
    analysis::types::{Type, SyscallSignature, SideEffect},
};

/// Comprehensive Neo N3 syscall database
#[derive(Debug, Clone)]
pub struct SyscallDatabase {
    /// Hash to syscall definition mapping
    syscalls_by_hash: HashMap<u32, SyscallDefinition>,
    /// Name to hash mapping for reverse lookup
    syscalls_by_name: HashMap<String, u32>,
    /// Compiled type signatures for fast lookup
    type_signatures: HashMap<u32, SyscallSignature>,
}

impl SyscallDatabase {
    /// Create a new syscall database with built-in Neo N3 syscalls
    pub fn new() -> Self {
        let mut database = Self {
            syscalls_by_hash: HashMap::new(),
            syscalls_by_name: HashMap::new(), 
            type_signatures: HashMap::new(),
        };
        
        // Load built-in Neo N3 syscalls
        database.load_builtin_syscalls();
        database
    }

    /// Load syscalls from configuration definitions
    pub fn from_definitions(definitions: Vec<SyscallDefinition>) -> Result<Self, ConfigError> {
        let mut database = Self::new();
        
        for def in definitions {
            database.add_syscall_definition(def)?;
        }
        
        Ok(database)
    }

    /// Add a syscall definition to the database
    pub fn add_syscall_definition(&mut self, def: SyscallDefinition) -> Result<(), ConfigError> {
        // Parse parameter types
        let mut param_types = Vec::new();
        for param_str in &def.parameters {
            param_types.push(self.parse_type_string(param_str)?);
        }

        // Parse return type
        let return_type = if let Some(return_str) = &def.return_type {
            if return_str == "Void" {
                Type::Void
            } else {
                self.parse_type_string(return_str)?
            }
        } else {
            Type::Void
        };

        // Parse effects
        let mut effects = Vec::new();
        for effect_str in &def.effects {
            effects.push(self.parse_effect_string(effect_str)?);
        }

        // Create type signature
        let signature = SyscallSignature {
            name: def.name.clone(),
            parameters: param_types,
            return_type,
            effects,
        };

        // Store in database
        self.syscalls_by_hash.insert(def.hash, def.clone());
        self.syscalls_by_name.insert(def.name.clone(), def.hash);
        self.type_signatures.insert(def.hash, signature);

        Ok(())
    }

    /// Get syscall definition by hash
    pub fn get_by_hash(&self, hash: u32) -> Option<&SyscallDefinition> {
        self.syscalls_by_hash.get(&hash)
    }

    /// Get syscall definition by name
    pub fn get_by_name(&self, name: &str) -> Option<&SyscallDefinition> {
        self.syscalls_by_name.get(name)
            .and_then(|hash| self.syscalls_by_hash.get(hash))
    }

    /// Get syscall type signature by hash
    pub fn get_signature(&self, hash: u32) -> Option<&SyscallSignature> {
        self.type_signatures.get(&hash)
    }

    /// Resolve syscall hash to name
    pub fn resolve_name(&self, hash: u32) -> String {
        self.syscalls_by_hash
            .get(&hash)
            .map(|def| def.name.clone())
            .unwrap_or_else(|| format!("syscall_{:08x}", hash))
    }

    /// Get argument count for a syscall
    pub fn get_arg_count(&self, hash: u32) -> usize {
        self.syscalls_by_hash
            .get(&hash)
            .map(|def| def.parameters.len())
            .unwrap_or(0)
    }

    /// Check if syscall returns a value
    pub fn returns_value(&self, hash: u32) -> bool {
        self.syscalls_by_hash
            .get(&hash)
            .map(|def| def.return_type.is_some() && def.return_type.as_ref().unwrap() != "Void")
            .unwrap_or(false)
    }

    /// Get syscall effects
    pub fn get_effects(&self, hash: u32) -> Vec<SideEffect> {
        self.type_signatures
            .get(&hash)
            .map(|sig| sig.effects.clone())
            .unwrap_or_default()
    }

    /// Get all known syscall hashes
    pub fn get_all_hashes(&self) -> Vec<u32> {
        self.syscalls_by_hash.keys().copied().collect()
    }

    /// Load built-in Neo N3 syscalls with correct hashes from the configuration
    fn load_builtin_syscalls(&mut self) {
        let builtin_syscalls = vec![
            // Runtime syscalls
            SyscallDefinition {
                name: "System.Runtime.Platform".to_string(),
                hash: 0x49de7d57,
                parameters: vec![],
                return_type: Some("String".to_string()),
                effects: vec!["SystemStateRead".to_string()],
                gas_cost: Some(250),
                description: Some("Gets the name of the current platform".to_string()),
            },
            SyscallDefinition {
                name: "System.Runtime.GetTrigger".to_string(),
                hash: 0x2d43a8aa,
                parameters: vec![],
                return_type: Some("Byte".to_string()),
                effects: vec!["SystemStateRead".to_string()],
                gas_cost: Some(250),
                description: Some("Gets the trigger type of the current execution".to_string()),
            },
            SyscallDefinition {
                name: "System.Runtime.GetTime".to_string(),
                hash: 0xf827ec8e,
                parameters: vec![],
                return_type: Some("UInteger".to_string()),
                effects: vec!["SystemStateRead".to_string()],
                gas_cost: Some(250),
                description: Some("Gets the timestamp of the current block".to_string()),
            },
            SyscallDefinition {
                name: "System.Runtime.GetExecutingScriptHash".to_string(),
                hash: 0x5d97c1b2,
                parameters: vec![],
                return_type: Some("Hash160".to_string()),
                effects: vec!["Pure".to_string()],
                gas_cost: Some(400),
                description: Some("Gets the script hash of the current contract".to_string()),
            },
            SyscallDefinition {
                name: "System.Runtime.GetCallingScriptHash".to_string(),
                hash: 0x91f9b23b,
                parameters: vec![],
                return_type: Some("Hash160".to_string()),
                effects: vec!["Pure".to_string()],
                gas_cost: Some(400),
                description: Some("Gets the script hash of the calling contract".to_string()),
            },
            SyscallDefinition {
                name: "System.Runtime.GetEntryScriptHash".to_string(),
                hash: 0x9e29b9a8,
                parameters: vec![],
                return_type: Some("Hash160".to_string()),
                effects: vec!["Pure".to_string()],
                gas_cost: Some(400),
                description: Some("Gets the script hash of the entry contract".to_string()),
            },

            // Storage syscalls
            SyscallDefinition {
                name: "System.Storage.GetContext".to_string(),
                hash: 0x9c7c9598,
                parameters: vec![],
                return_type: Some("StorageContext".to_string()),
                effects: vec!["Pure".to_string()],
                gas_cost: Some(400),
                description: Some("Gets the storage context of the current contract".to_string()),
            },
            SyscallDefinition {
                name: "System.Storage.GetReadOnlyContext".to_string(),
                hash: 0xe1c83c39,
                parameters: vec![],
                return_type: Some("StorageContext".to_string()),
                effects: vec!["Pure".to_string()],
                gas_cost: Some(400),
                description: Some("Gets the read-only storage context of the current contract".to_string()),
            },
            SyscallDefinition {
                name: "System.Storage.Get".to_string(),
                hash: 0x925de831,
                parameters: vec!["StorageContext".to_string(), "ByteArray".to_string()],
                return_type: Some("ByteArray".to_string()),
                effects: vec!["StorageRead".to_string()],
                gas_cost: Some(1000000),
                description: Some("Gets a value from storage".to_string()),
            },
            SyscallDefinition {
                name: "System.Storage.Put".to_string(),
                hash: 0xe63f1884,
                parameters: vec!["StorageContext".to_string(), "ByteArray".to_string(), "ByteArray".to_string()],
                return_type: Some("Void".to_string()),
                effects: vec!["StorageWrite".to_string()],
                gas_cost: Some(0),
                description: Some("Puts a value into storage".to_string()),
            },
            SyscallDefinition {
                name: "System.Storage.Delete".to_string(),
                hash: 0x7ce2e494,
                parameters: vec!["StorageContext".to_string(), "ByteArray".to_string()],
                return_type: Some("Void".to_string()),
                effects: vec!["StorageWrite".to_string()],
                gas_cost: Some(1000000),
                description: Some("Deletes a value from storage".to_string()),
            },
            SyscallDefinition {
                name: "System.Storage.Find".to_string(),
                hash: 0xa09b1eef,
                parameters: vec!["StorageContext".to_string(), "ByteArray".to_string(), "Byte".to_string()],
                return_type: Some("Iterator".to_string()),
                effects: vec!["StorageRead".to_string()],
                gas_cost: Some(1000000),
                description: Some("Finds storage entries with the given prefix".to_string()),
            },

            // Contract syscalls
            SyscallDefinition {
                name: "System.Contract.Call".to_string(),
                hash: 0x627d5b52,
                parameters: vec!["Hash160".to_string(), "String".to_string(), "Array".to_string(), "CallFlags".to_string()],
                return_type: Some("Any".to_string()),
                effects: vec!["ContractCall".to_string()],
                gas_cost: Some(0),
                description: Some("Calls another contract".to_string()),
            },
            SyscallDefinition {
                name: "System.Contract.CallEx".to_string(),
                hash: 0x14e12327,
                parameters: vec!["Hash160".to_string(), "String".to_string(), "Array".to_string(), "CallFlags".to_string()],
                return_type: Some("Any".to_string()),
                effects: vec!["ContractCall".to_string()],
                gas_cost: Some(0),
                description: Some("Calls another contract with extended functionality".to_string()),
            },

            // Crypto syscalls
            SyscallDefinition {
                name: "System.Crypto.CheckSig".to_string(),
                hash: 0x82958f5a,
                parameters: vec!["ByteArray".to_string(), "ECPoint".to_string()],
                return_type: Some("Boolean".to_string()),
                effects: vec!["Pure".to_string()],
                gas_cost: Some(1000000),
                description: Some("Verifies a signature".to_string()),
            },
            SyscallDefinition {
                name: "System.Crypto.CheckMultisig".to_string(),
                hash: 0xf60652e8,
                parameters: vec!["Array".to_string(), "Array".to_string()],
                return_type: Some("Boolean".to_string()),
                effects: vec!["Pure".to_string()],
                gas_cost: Some(0),
                description: Some("Verifies multiple signatures".to_string()),
            },

            // Iterator syscalls
            SyscallDefinition {
                name: "System.Iterator.Next".to_string(),
                hash: 0x7e6a2bb7,
                parameters: vec!["Iterator".to_string()],
                return_type: Some("Boolean".to_string()),
                effects: vec!["Pure".to_string()],
                gas_cost: Some(1000000),
                description: Some("Moves to the next item in an iterator".to_string()),
            },
            SyscallDefinition {
                name: "System.Iterator.Value".to_string(),
                hash: 0x63b6c5ee,
                parameters: vec!["Iterator".to_string()],
                return_type: Some("Array".to_string()),
                effects: vec!["Pure".to_string()],
                gas_cost: Some(400),
                description: Some("Gets the current value from an iterator".to_string()),
            },

            // JSON syscalls
            SyscallDefinition {
                name: "System.Json.Serialize".to_string(),
                hash: 0xa0ab5461,
                parameters: vec!["Any".to_string()],
                return_type: Some("ByteArray".to_string()),
                effects: vec!["Pure".to_string()],
                gas_cost: Some(100000),
                description: Some("Serializes an object to JSON".to_string()),
            },
            SyscallDefinition {
                name: "System.Json.Deserialize".to_string(),
                hash: 0x7d4b2a25,
                parameters: vec!["ByteArray".to_string()],
                return_type: Some("Any".to_string()),
                effects: vec!["Pure".to_string()],
                gas_cost: Some(500000),
                description: Some("Deserializes JSON to an object".to_string()),
            },
        ];

        // Add all builtin syscalls
        for def in builtin_syscalls {
            self.add_syscall_definition(def).unwrap();
        }
    }

    /// Parse type string to Type enum
    fn parse_type_string(&self, type_str: &str) -> Result<Type, ConfigError> {
        use crate::analysis::types::{PrimitiveType};

        match type_str {
            "Void" => Ok(Type::Void),
            "Boolean" => Ok(Type::Primitive(PrimitiveType::Boolean)),
            "Byte" => Ok(Type::Primitive(PrimitiveType::Integer)),
            "Integer" => Ok(Type::Primitive(PrimitiveType::Integer)),
            "UInteger" => Ok(Type::Primitive(PrimitiveType::Integer)),
            "String" => Ok(Type::Primitive(PrimitiveType::String)),
            "ByteArray" => Ok(Type::Primitive(PrimitiveType::ByteArray)),
            "Hash160" => Ok(Type::Primitive(PrimitiveType::ByteArray)),
            "Hash256" => Ok(Type::Primitive(PrimitiveType::ByteArray)),
            "ECPoint" => Ok(Type::Primitive(PrimitiveType::ByteArray)),
            "Array" => Ok(Type::Array(Box::new(Type::Unknown))),
            "Any" => Ok(Type::Unknown),
            "StorageContext" => Ok(Type::Unknown), // Opaque type
            "Iterator" => Ok(Type::Unknown), // Opaque type
            "InteropInterface" => Ok(Type::Unknown), // Opaque type
            "CallFlags" => Ok(Type::Primitive(PrimitiveType::Integer)),
            _ => {
                // Try to handle complex types or fall back to Unknown
                Ok(Type::Unknown)
            }
        }
    }

    /// Parse effect string to SideEffect enum
    fn parse_effect_string(&self, effect_str: &str) -> Result<SideEffect, ConfigError> {
        match effect_str {
            "Pure" => Ok(SideEffect::Pure),
            "StorageRead" => Ok(SideEffect::StorageRead),
            "StorageWrite" => Ok(SideEffect::StorageWrite),
            "ContractCall" => Ok(SideEffect::ContractCall),
            "EventEmit" => Ok(SideEffect::EventEmit),
            "StateChange" | "SystemStateRead" => Ok(SideEffect::StateChange),
            _ => {
                // Unknown effect - treat as state change for safety
                Ok(SideEffect::StateChange)
            }
        }
    }
}

impl Default for SyscallDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syscall_database_basic() {
        let db = SyscallDatabase::new();
        
        // Test that we can resolve known syscalls
        assert_eq!(db.resolve_name(0x925de831), "System.Storage.Get");
        assert_eq!(db.resolve_name(0xe63f1884), "System.Storage.Put");
        
        // Test argument counting
        assert_eq!(db.get_arg_count(0x925de831), 2); // Storage.Get takes context + key
        assert_eq!(db.get_arg_count(0xe63f1884), 3); // Storage.Put takes context + key + value
        
        // Test return value detection
        assert_eq!(db.returns_value(0x925de831), true);  // Storage.Get returns value
        assert_eq!(db.returns_value(0xe63f1884), false); // Storage.Put returns void
    }

    #[test]
    fn test_syscall_type_signatures() {
        let db = SyscallDatabase::new();
        
        // Test type signature retrieval
        let sig = db.get_signature(0x925de831).unwrap(); // System.Storage.Get
        assert_eq!(sig.name, "System.Storage.Get");
        assert_eq!(sig.parameters.len(), 2);
        assert!(!matches!(sig.return_type, Type::Void));
        assert!(sig.effects.contains(&SideEffect::StorageRead));
    }

    #[test] 
    fn test_unknown_syscalls() {
        let db = SyscallDatabase::new();
        
        // Test handling of unknown syscalls
        assert_eq!(db.resolve_name(0xdeadbeef), "syscall_deadbeef");
        assert_eq!(db.get_arg_count(0xdeadbeef), 0);
        assert_eq!(db.returns_value(0xdeadbeef), false);
    }
}