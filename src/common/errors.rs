//! Error types and handling for the Neo N3 decompiler

use thiserror::Error;

/// Main result type for decompiler operations
pub type DecompilerResult<T> = Result<T, DecompilerError>;

/// Main error type encompassing all decompiler errors
#[derive(Error, Debug)]
pub enum DecompilerError {
    #[error("NEF parsing error: {0}")]
    NEFParse(#[from] NEFParseError),

    #[error("Manifest parsing error: {0}")]
    ManifestParse(#[from] ManifestParseError),

    #[error("Disassembly error: {0}")]
    Disassembly(#[from] DisassemblyError),

    #[error("IR lifting error: {0}")]
    Lifting(#[from] LiftError),

    #[error("Type inference error: {0}")]
    TypeInference(#[from] TypeInferenceError),

    #[error("Analysis error: {0}")]
    Analysis(#[from] AnalysisError),

    #[error("Code generation error: {0}")]
    CodeGeneration(#[from] CodeGenerationError),

    #[error("Plugin error: {0}")]
    Plugin(#[from] PluginError),

    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// NEF file parsing errors
#[derive(Error, Debug)]
pub enum NEFParseError {
    #[error("Invalid NEF magic bytes")]
    InvalidMagic,

    #[error("Unsupported NEF version: {version}")]
    UnsupportedVersion { version: u32 },

    #[error("Invalid checksum: expected {expected:08x}, got {actual:08x}")]
    InvalidChecksum { expected: u32, actual: u32 },

    #[error("Truncated NEF file: expected {expected} bytes, got {actual}")]
    TruncatedFile { expected: usize, actual: usize },

    #[error("Invalid method token at offset {offset}")]
    InvalidMethodToken { offset: usize },

    #[error("Invalid bytecode section")]
    InvalidBytecode,
}

/// Contract manifest parsing errors
#[derive(Error, Debug)]
pub enum ManifestParseError {
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Missing required field: {field}")]
    MissingField { field: String },

    #[error("Invalid ABI format")]
    InvalidABI,

    #[error("Invalid permission format")]
    InvalidPermission,

    #[error("Invalid group format")]
    InvalidGroup,
}

/// Bytecode disassembly errors
#[derive(Error, Debug)]
pub enum DisassemblyError {
    #[error("Unknown opcode: 0x{opcode:02x} at offset {offset}")]
    UnknownOpcode { opcode: u8, offset: u32 },

    #[error("Invalid operand for opcode {opcode} at offset {offset}")]
    InvalidOperand { opcode: String, offset: u32 },

    #[error("Invalid operand: expected {expected} at offset {offset}")]
    InvalidOperandType { expected: String, offset: u32 },

    #[error("Truncated instruction at offset {offset}")]
    TruncatedInstruction { offset: u32 },

    #[error("Invalid jump target: {target} at offset {offset}")]
    InvalidJumpTarget { target: i32, offset: u32 },

    #[error("Invalid syscall hash: 0x{hash:08x} at offset {offset}")]
    InvalidSyscallHash { hash: u32, offset: u32 },
}

/// IR lifting errors
#[derive(Error, Debug)]
pub enum LiftError {
    #[error("Cannot lift instruction {opcode:?} at offset {offset}")]
    UnsupportedInstruction { opcode: String, offset: u32 },

    #[error("Stack underflow when lifting instruction at offset {offset}")]
    StackUnderflow { offset: u32 },

    #[error("Invalid control flow at offset {offset}")]
    InvalidControlFlow { offset: u32 },

    #[error("Unresolved jump target: {target}")]
    UnresolvedJumpTarget { target: i32 },

    #[error("Invalid operand for instruction at offset {offset}")]
    InvalidOperand { offset: u32 },
}

/// Type inference errors
#[derive(Error, Debug)]
pub enum TypeInferenceError {
    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeMismatch { expected: String, actual: String },

    #[error("Unresolvable type constraint")]
    UnresolvableConstraint,

    #[error("Circular type dependency")]
    CircularDependency,

    #[error("Unknown type: {type_name}")]
    UnknownType { type_name: String },

    #[error("Type inference timeout")]
    Timeout,
}

/// Analysis pass errors
#[derive(Error, Debug)]
pub enum AnalysisError {
    #[error("Control flow analysis failed: {reason}")]
    ControlFlow { reason: String },

    #[error("Data flow analysis failed: {reason}")]
    DataFlow { reason: String },

    #[error("Effect analysis failed: {reason}")]
    Effect { reason: String },

    #[error("Analysis timeout for pass: {pass_name}")]
    Timeout { pass_name: String },

    #[error("Analysis dependency not satisfied: {dependency}")]
    MissingDependency { dependency: String },

    #[error("CFG construction error: {0}")]
    CFGError(String),

    #[error("Analysis failure: {0}")]
    AnalysisFailure(String),

    #[error("Invalid contract hash: expected {expected_length} bytes, got {actual_length}")]
    InvalidContractHash { expected_length: usize, actual_length: usize },
}

/// Code generation errors
#[derive(Error, Debug)]
pub enum CodeGenerationError {
    #[error("Cannot generate code for IR node: {node_type}")]
    UnsupportedIRNode { node_type: String },

    #[error("Invalid expression tree")]
    InvalidExpression,

    #[error("Missing symbol information for: {symbol}")]
    MissingSymbol { symbol: String },

    #[error("Invalid operation: {operation}")]
    InvalidOperation { operation: String },

    #[error("Unsupported feature: {feature}")]
    UnsupportedFeature { feature: String },

    #[error("Code generation timeout")]
    Timeout,
}

/// Plugin system errors
#[derive(Error, Debug)]
pub enum PluginError {
    #[error("Plugin not found: {name}")]
    NotFound { name: String },

    #[error("Plugin loading failed: {name} - {reason}")]
    LoadFailed { name: String, reason: String },

    #[error("Plugin initialization failed: {name} - {reason}")]
    InitializationFailed { name: String, reason: String },

    #[error("Plugin execution failed: {name} - {reason}")]
    ExecutionFailed { name: String, reason: String },

    #[error("Plugin dependency not satisfied: {plugin} requires {dependency}")]
    DependencyNotSatisfied { plugin: String, dependency: String },

    #[error("Plugin API version mismatch: expected {expected}, got {actual}")]
    APIVersionMismatch { expected: String, actual: String },
}

/// Configuration errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration file not found: {path}")]
    FileNotFound { path: String },

    #[error("Invalid configuration format: {reason}")]
    InvalidFormat { reason: String },

    #[error("Missing required configuration: {key}")]
    MissingConfiguration { key: String },

    #[error("Invalid configuration value for {key}: {value}")]
    InvalidValue { key: String, value: String },

    #[error("TOML parsing error: {0}")]
    Toml(#[from] toml::de::Error),
}

impl DecompilerError {
    /// Create an internal error with custom message
    pub fn internal<T: Into<String>>(msg: T) -> Self {
        DecompilerError::Internal(msg.into())
    }

    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            DecompilerError::NEFParse(e) => e.is_recoverable(),
            DecompilerError::Disassembly(e) => e.is_recoverable(),
            DecompilerError::TypeInference(_) => true,
            DecompilerError::Analysis(_) => true,
            DecompilerError::Plugin(_) => true,
            _ => false,
        }
    }

    /// Get error severity level
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            DecompilerError::NEFParse(NEFParseError::InvalidMagic) => ErrorSeverity::Critical,
            DecompilerError::NEFParse(NEFParseError::InvalidChecksum { .. }) => ErrorSeverity::High,
            DecompilerError::Disassembly(DisassemblyError::UnknownOpcode { .. }) => ErrorSeverity::Medium,
            DecompilerError::TypeInference(_) => ErrorSeverity::Medium,
            DecompilerError::Analysis(_) => ErrorSeverity::Low,
            DecompilerError::Plugin(_) => ErrorSeverity::Low,
            _ => ErrorSeverity::High,
        }
    }
}

impl NEFParseError {
    fn is_recoverable(&self) -> bool {
        matches!(self, 
            NEFParseError::InvalidMethodToken { .. } | 
            NEFParseError::UnsupportedVersion { .. }
        )
    }
}

impl DisassemblyError {
    fn is_recoverable(&self) -> bool {
        matches!(self, 
            DisassemblyError::UnknownOpcode { .. } |
            DisassemblyError::InvalidSyscallHash { .. }
        )
    }
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorSeverity::Low => write!(f, "LOW"),
            ErrorSeverity::Medium => write!(f, "MEDIUM"),
            ErrorSeverity::High => write!(f, "HIGH"),
            ErrorSeverity::Critical => write!(f, "CRITICAL"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_recovery() {
        let recoverable = DecompilerError::TypeInference(
            TypeInferenceError::TypeMismatch {
                expected: "int".to_string(),
                actual: "string".to_string(),
            }
        );
        assert!(recoverable.is_recoverable());

        let non_recoverable = DecompilerError::NEFParse(NEFParseError::InvalidMagic);
        assert!(!non_recoverable.is_recoverable());
    }

    #[test]
    fn test_error_severity() {
        let critical = DecompilerError::NEFParse(NEFParseError::InvalidMagic);
        assert_eq!(critical.severity(), ErrorSeverity::Critical);

        let medium = DecompilerError::TypeInference(TypeInferenceError::UnresolvableConstraint);
        assert_eq!(medium.severity(), ErrorSeverity::Medium);
    }
}