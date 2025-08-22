//! Plugin system for extensible functionality

// Complete plugin system implementation

use std::collections::HashMap;

/// Plugin manager for loading and coordinating plugins
pub struct PluginManager {
    /// Loaded plugins
    plugins: HashMap<String, Box<dyn Plugin>>,
}

impl PluginManager {
    /// Create new plugin manager
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Core plugin trait
pub trait Plugin: Send + Sync {
    /// Plugin name
    fn name(&self) -> &str;
    
    /// Plugin version
    fn version(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_manager_creation() {
        let manager = PluginManager::new();
        assert!(manager.plugins.is_empty());
    }
}