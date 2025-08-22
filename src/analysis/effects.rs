//! Effect system for tracking side effects

use crate::common::types::*;
use std::collections::HashMap;

/// Effect system for tracking side effects
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Effect {
    /// Storage read operation
    StorageRead { key_pattern: KeyPattern },
    /// Storage write operation  
    StorageWrite { key_pattern: KeyPattern },
    /// Contract invocation
    ContractCall { 
        contract: ContractId, 
        method: String,
        effects: Vec<Effect>,
    },
    /// Event emission
    EventEmit { event_name: String },
    /// Neo transfer
    Transfer {
        from: Option<Hash160>,
        to: Option<Hash160>, 
        amount: Option<u64>,
    },
    /// Gas consumption
    GasConsumption { amount: u64 },
    /// Random number generation
    RandomAccess,
    /// System state access
    SystemStateRead,
    /// Network communication
    NetworkAccess,
    /// State change
    StateChange,
    /// No side effects
    Pure,
}

/// Storage key pattern matching
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum KeyPattern {
    /// Exact key match
    Exact(Vec<u8>),
    /// Key prefix match
    Prefix(Vec<u8>),
    /// Wildcard pattern
    Wildcard,
    /// Parameterized key
    Parameterized(String),
    /// Dynamic key pattern
    Dynamic,
}

/// Effect inference engine
pub struct EffectInferenceEngine {
    /// Known syscall effects
    pub syscall_effects: HashMap<String, Vec<Effect>>,
    /// Contract interface effects
    pub interface_effects: HashMap<String, Vec<Effect>>,
}

impl EffectInferenceEngine {
    /// Create new effect inference engine
    pub fn new() -> Self {
        let mut syscall_effects = HashMap::new();
        
        // Initialize common syscall effects
        syscall_effects.insert(
            "System.Storage.Get".to_string(),
            vec![Effect::StorageRead { key_pattern: KeyPattern::Wildcard }]
        );
        
        syscall_effects.insert(
            "System.Storage.Put".to_string(),
            vec![Effect::StorageWrite { key_pattern: KeyPattern::Wildcard }]
        );
        
        syscall_effects.insert(
            "System.Runtime.Notify".to_string(),
            vec![Effect::EventEmit { event_name: "Notification".to_string() }]
        );

        Self {
            syscall_effects,
            interface_effects: HashMap::new(),
        }
    }

    /// Infer effects for a syscall
    pub fn infer_syscall_effects(&self, syscall_name: &str) -> Vec<Effect> {
        self.syscall_effects
            .get(syscall_name)
            .cloned()
            .unwrap_or_else(|| vec![Effect::Pure])
    }

    /// Combine multiple effects
    pub fn combine_effects(&self, effects: Vec<Vec<Effect>>) -> Vec<Effect> {
        let mut combined = Vec::new();
        for effect_list in effects {
            combined.extend(effect_list);
        }
        
        // Remove duplicates and merge similar effects
        self.deduplicate_effects(combined)
    }

    /// Remove duplicate effects
    pub fn deduplicate_effects(&self, effects: Vec<Effect>) -> Vec<Effect> {
        let mut unique_effects = Vec::new();
        let mut seen = std::collections::HashSet::new();
        
        for effect in effects {
            if seen.insert(effect.clone()) {
                unique_effects.push(effect);
            }
        }
        
        unique_effects
    }
}

impl Default for EffectInferenceEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect {
    /// Check if effect is pure (no side effects)
    pub fn is_pure(&self) -> bool {
        matches!(self, Effect::Pure)
    }

    /// Check if effect involves storage
    pub fn is_storage_effect(&self) -> bool {
        matches!(self, Effect::StorageRead { .. } | Effect::StorageWrite { .. })
    }

    /// Get effect severity (for risk analysis)
    pub fn severity(&self) -> u32 {
        match self {
            Effect::Pure => 0,
            Effect::StorageRead { .. } => 1,
            Effect::SystemStateRead => 1,
            Effect::EventEmit { .. } => 2,
            Effect::StorageWrite { .. } => 3,
            Effect::GasConsumption { .. } => 3,
            Effect::StateChange => 4,
            Effect::ContractCall { .. } => 4,
            Effect::Transfer { .. } => 5,
            Effect::RandomAccess => 6,
            Effect::NetworkAccess => 7,
        }
    }
}

impl KeyPattern {
    /// Check if pattern matches a key
    pub fn matches(&self, key: &[u8]) -> bool {
        match self {
            KeyPattern::Exact(pattern) => pattern == key,
            KeyPattern::Prefix(prefix) => key.starts_with(prefix),
            KeyPattern::Wildcard => true,
            KeyPattern::Parameterized(_) => true, // Requires pattern template matching
            KeyPattern::Dynamic => true, // Dynamic patterns match all keys
        }
    }

    /// Create prefix pattern
    pub fn prefix(prefix: Vec<u8>) -> Self {
        KeyPattern::Prefix(prefix)
    }

    /// Create exact pattern
    pub fn exact(key: Vec<u8>) -> Self {
        KeyPattern::Exact(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effect_properties() {
        let pure_effect = Effect::Pure;
        let storage_read = Effect::StorageRead {
            key_pattern: KeyPattern::Wildcard
        };
        let storage_write = Effect::StorageWrite {
            key_pattern: KeyPattern::Wildcard
        };

        assert!(pure_effect.is_pure());
        assert!(!storage_read.is_pure());
        assert!(storage_read.is_storage_effect());
        assert!(storage_write.is_storage_effect());
        
        assert_eq!(pure_effect.severity(), 0);
        assert!(storage_write.severity() > storage_read.severity());
    }

    #[test]
    fn test_key_pattern_matching() {
        let exact_pattern = KeyPattern::exact(b"test_key".to_vec());
        let prefix_pattern = KeyPattern::prefix(b"test_".to_vec());
        let wildcard_pattern = KeyPattern::Wildcard;

        assert!(exact_pattern.matches(b"test_key"));
        assert!(!exact_pattern.matches(b"other_key"));
        
        assert!(prefix_pattern.matches(b"test_key"));
        assert!(prefix_pattern.matches(b"test_value"));
        assert!(!prefix_pattern.matches(b"other_key"));
        
        assert!(wildcard_pattern.matches(b"any_key"));
        assert!(wildcard_pattern.matches(b""));
    }

    #[test]
    fn test_effect_inference_engine() {
        let engine = EffectInferenceEngine::new();
        
        let storage_effects = engine.infer_syscall_effects("System.Storage.Get");
        assert!(!storage_effects.is_empty());
        assert!(storage_effects.iter().any(|e| e.is_storage_effect()));
        
        let unknown_effects = engine.infer_syscall_effects("Unknown.Syscall");
        assert_eq!(unknown_effects, vec![Effect::Pure]);
    }

    #[test]
    fn test_combine_effects() {
        let engine = EffectInferenceEngine::new();
        
        let effects1 = vec![Effect::Pure];
        let effects2 = vec![
            Effect::StorageRead { key_pattern: KeyPattern::Wildcard },
            Effect::Pure, // Duplicate
        ];
        
        let combined = engine.combine_effects(vec![effects1, effects2]);
        assert_eq!(combined.len(), 2); // Pure and StorageRead, duplicates removed
    }
}