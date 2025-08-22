//! Configuration system for the Neo N3 decompiler

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use crate::common::errors::ConfigError;

/// Main decompiler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompilerConfig {
    /// Analysis configuration
    pub analysis: AnalysisConfig,
    
    /// Output configuration
    pub output: OutputConfig,
    
    /// Plugin configuration
    pub plugins: PluginConfig,
    
    /// Performance tuning
    pub performance: PerformanceConfig,

    /// Syscall definitions
    pub syscalls: SyscallConfig,
}

impl DecompilerConfig {
    /// Load configuration from file
    pub fn load_from_file(path: &std::path::Path) -> Result<Self, ConfigError> {
        ConfigLoader::load_from_file(path)
    }
}

impl Default for DecompilerConfig {
    fn default() -> Self {
        Self {
            analysis: AnalysisConfig::default(),
            output: OutputConfig::default(),
            plugins: PluginConfig::default(),
            performance: PerformanceConfig::default(),
            syscalls: SyscallConfig::default(),
        }
    }
}

/// Analysis pass configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    /// Enable type inference
    pub enable_type_inference: bool,
    
    /// Enable effect analysis
    pub enable_effect_analysis: bool,

    /// Enable control flow analysis
    pub enable_cfg_analysis: bool,

    /// Enable loop detection
    pub enable_loop_detection: bool,

    /// Enable dead code elimination
    pub enable_dead_code_elimination: bool,
    
    /// Maximum analysis depth
    pub max_analysis_depth: u32,
    
    /// Timeout for analysis passes (seconds)
    pub analysis_timeout: u64,

    /// Parallel analysis execution
    pub parallel_analysis: bool,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            enable_type_inference: true,
            enable_effect_analysis: true,
            enable_cfg_analysis: true,
            enable_loop_detection: true,
            enable_dead_code_elimination: false,
            max_analysis_depth: 100,
            analysis_timeout: 30,
            parallel_analysis: true,
        }
    }
}

/// Output generation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Pseudocode syntax style
    pub syntax_style: SyntaxStyle,

    /// Include type annotations in output
    pub include_type_annotations: bool,

    /// Include IR comments in pseudocode
    pub include_ir_comments: bool,

    /// Indentation size
    pub indent_size: usize,

    /// Maximum line length
    pub max_line_length: usize,

    /// Generate detailed reports
    pub generate_reports: bool,

    /// Include performance metrics
    pub include_performance_metrics: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            syntax_style: SyntaxStyle::CStyle,
            include_type_annotations: true,
            include_ir_comments: false,
            indent_size: 4,
            max_line_length: 100,
            generate_reports: false,
            include_performance_metrics: false,
        }
    }
}

/// Supported pseudocode syntax styles
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SyntaxStyle {
    CStyle,      // C/Java-like syntax
    Python,      // Python-like syntax  
    Rust,        // Rust-like syntax
    TypeScript,  // TypeScript-like syntax
}

/// Plugin system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Enable plugin system
    pub enabled: bool,

    /// Plugin search paths
    pub plugin_paths: Vec<PathBuf>,

    /// Enabled plugins
    pub enabled_plugins: Vec<String>,

    /// Plugin-specific configurations
    pub plugin_settings: HashMap<String, toml::Value>,

    /// Plugin execution timeout (seconds)
    pub plugin_timeout: u64,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            plugin_paths: vec![
                PathBuf::from("./plugins"),
                PathBuf::from("~/.neo-decompiler/plugins"),
            ],
            enabled_plugins: vec![
                "syscall_analyzer".to_string(),
                "nep_detector".to_string(),
            ],
            plugin_settings: HashMap::new(),
            plugin_timeout: 10,
        }
    }
}

/// Performance optimization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Enable parallel processing
    pub parallel_processing: bool,

    /// Number of worker threads
    pub worker_threads: Option<usize>,

    /// Memory limit (MB)
    pub memory_limit_mb: Option<usize>,

    /// Enable caching
    pub enable_caching: bool,

    /// Cache size limit (entries)
    pub cache_size_limit: usize,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            parallel_processing: true,
            worker_threads: None, // Use system default
            memory_limit_mb: Some(1024), // 1GB limit
            enable_caching: true,
            cache_size_limit: 10000,
        }
    }
}

/// Syscall definitions configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyscallConfig {
    /// Path to syscall definition files
    pub syscall_definitions_path: PathBuf,

    /// Custom syscall definitions
    pub custom_syscalls: HashMap<u32, SyscallDefinition>,

    /// Enable syscall effect analysis
    pub enable_effect_analysis: bool,
}

impl Default for SyscallConfig {
    fn default() -> Self {
        Self {
            syscall_definitions_path: PathBuf::from("./config/syscalls"),
            custom_syscalls: HashMap::new(),
            enable_effect_analysis: true,
        }
    }
}

/// Individual syscall definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyscallDefinition {
    /// Syscall name
    pub name: String,

    /// Syscall hash/ID
    pub hash: u32,

    /// Parameter types
    pub parameters: Vec<String>,

    /// Return type
    pub return_type: Option<String>,

    /// Side effects
    pub effects: Vec<String>,

    /// Gas cost
    pub gas_cost: Option<u64>,

    /// Description
    pub description: Option<String>,
}

/// Configuration loader
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load configuration from file
    pub fn load_from_file(path: &std::path::Path) -> Result<DecompilerConfig, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::FileNotFound { path: path.to_string_lossy().to_string() })?;
        let config: DecompilerConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Load configuration from multiple sources with priority
    pub fn load() -> Result<DecompilerConfig, ConfigError> {
        let mut config = DecompilerConfig::default();

        // Try loading from standard locations
        let config_paths = [
            "./decompiler.toml",
            "./config/decompiler.toml",
            "~/.neo-decompiler/config.toml",
        ];

        for path in &config_paths {
            let path = std::path::Path::new(path);
            if path.exists() {
                let file_config = Self::load_from_file(path)?;
                config = Self::merge_configs(config, file_config);
                break;
            }
        }

        // Override with environment variables
        config = Self::apply_env_overrides(config);

        Ok(config)
    }

    /// Load syscall definitions from TOML files
    pub fn load_syscall_definitions(path: &std::path::Path) -> Result<Vec<SyscallDefinition>, ConfigError> {
        let mut definitions = Vec::new();

        if path.is_dir() {
            for entry in std::fs::read_dir(path).map_err(|e| ConfigError::FileNotFound { path: path.to_string_lossy().to_string() })? {
                let entry = entry.map_err(|e| ConfigError::InvalidFormat { reason: e.to_string() })?;
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "toml") {
                    let content = std::fs::read_to_string(&path).map_err(|e| ConfigError::FileNotFound { path: path.to_string_lossy().to_string() })?;
                    let mut file_definitions: Vec<SyscallDefinition> = toml::from_str(&content)?;
                    definitions.append(&mut file_definitions);
                }
            }
        }

        Ok(definitions)
    }

    /// Merge two configurations, with `override_config` taking precedence
    fn merge_configs(base: DecompilerConfig, override_config: DecompilerConfig) -> DecompilerConfig {
        // Merge configuration values with precedence
        override_config
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(mut config: DecompilerConfig) -> DecompilerConfig {
        if let Ok(value) = std::env::var("NEO_DECOMPILER_PARALLEL") {
            config.performance.parallel_processing = value.parse().unwrap_or(true);
        }

        if let Ok(value) = std::env::var("NEO_DECOMPILER_THREADS") {
            config.performance.worker_threads = value.parse().ok();
        }

        if let Ok(value) = std::env::var("NEO_DECOMPILER_MEMORY_LIMIT") {
            config.performance.memory_limit_mb = value.parse().ok();
        }

        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DecompilerConfig::default();
        assert!(config.analysis.enable_type_inference);
        assert!(config.performance.parallel_processing);
        assert_eq!(config.output.syntax_style as u8, SyntaxStyle::CStyle as u8);
    }

    #[test]
    fn test_config_serialization() {
        let config = DecompilerConfig::default();
        let serialized = toml::to_string(&config).unwrap();
        let deserialized: DecompilerConfig = toml::from_str(&serialized).unwrap();
        
        // Compare some key fields
        assert_eq!(config.analysis.enable_type_inference, deserialized.analysis.enable_type_inference);
        assert_eq!(config.output.indent_size, deserialized.output.indent_size);
    }
}