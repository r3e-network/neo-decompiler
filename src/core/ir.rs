//! Intermediate Representation (IR) definitions for Neo N3 decompiler

use crate::analysis::{effects::KeyPattern, types::Type};
use crate::common::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// High-level intermediate representation function
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IRFunction {
    /// Function name
    pub name: String,
    /// Function parameters
    pub parameters: Vec<Parameter>,
    /// Local variables
    pub locals: Vec<LocalVariable>,
    /// Basic blocks
    pub blocks: HashMap<BlockId, IRBlock>,
    /// Entry block ID
    pub entry_block: BlockId,
    /// Exit blocks
    pub exit_blocks: Vec<BlockId>,
    /// Return type
    pub return_type: Option<Type>,
    /// Function metadata
    pub metadata: FunctionMetadata,
}

/// IR basic block
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IRBlock {
    /// Block identifier
    pub id: BlockId,
    /// Block operations
    pub operations: Vec<Operation>,
    /// Block terminator
    pub terminator: Terminator,
    /// Predecessor blocks
    pub predecessors: Vec<BlockId>,
    /// Successor blocks  
    pub successors: Vec<BlockId>,
}

/// Contract-level IR representation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IRContract {
    /// Contract functions
    pub functions: Vec<IRFunction>,
    /// Contract events
    pub events: Vec<EventDefinition>,
    /// Storage layout
    pub storage_layout: StorageLayout,
    /// Contract metadata
    pub metadata: ContractMetadata,
}

/// Individual IR operations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Operation {
    /// Variable assignment
    Assign {
        target: Variable,
        source: Expression,
    },
    /// Syscall invocation
    Syscall {
        name: String,
        arguments: Vec<Expression>,
        return_type: Option<Type>,
        target: Option<Variable>,
    },
    /// Contract call
    ContractCall {
        contract: Expression,
        method: String,
        arguments: Vec<Expression>,
        call_flags: u8,
        target: Option<Variable>,
    },
    /// Storage operation
    Storage {
        operation: StorageOp,
        key: Expression,
        value: Option<Expression>,
        target: Option<Variable>,
    },
    /// Stack operation
    Stack {
        operation: StackOp,
        operands: Vec<Expression>,
        target: Option<Variable>,
    },
    /// Arithmetic operation
    Arithmetic {
        operator: BinaryOperator,
        left: Expression,
        right: Expression,
        target: Variable,
    },
    /// Type conversion
    Convert {
        source: Expression,
        target_type: Type,
        target: Variable,
    },
    /// Unary operation
    Unary {
        operator: UnaryOperator,
        operand: Expression,
        target: Variable,
    },
    /// Builtin function call
    BuiltinCall {
        name: String,
        arguments: Vec<Expression>,
        target: Option<Variable>,
    },
    /// Array operation
    ArrayOp {
        operation: ArrayOperation,
        array: Expression,
        index: Option<Expression>,
        value: Option<Expression>,
        target: Option<Variable>,
    },
    /// Map operation
    MapOp {
        operation: MapOperation,
        map: Expression,
        key: Expression,
        value: Option<Expression>,
        target: Option<Variable>,
    },
    /// String operation
    StringOp {
        operation: StringOperation,
        operands: Vec<Expression>,
        target: Variable,
    },
    /// Type check operation
    TypeCheck {
        value: Expression,
        target_type: Type,
        target: Variable,
    },
    /// Throw exception
    Throw { exception: Expression },
    /// Assert condition
    Assert {
        condition: Expression,
        message: Option<Expression>,
    },
    /// Abort execution
    Abort { message: Option<Expression> },
    /// Effect annotation
    Effect {
        effect_type: EffectType,
        description: String,
    },
    /// Comment/annotation
    Comment(String),
}

/// Block termination conditions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Terminator {
    /// Unconditional jump
    Jump(BlockId),
    /// Conditional branch
    Branch {
        condition: Expression,
        true_target: BlockId,
        false_target: BlockId,
    },
    /// Return from function
    Return(Option<Expression>),
    /// Exception/abort
    Abort(Option<Expression>),
    /// Switch statement (for complex jumps)
    Switch {
        discriminant: Expression,
        targets: Vec<(Literal, BlockId)>,
        default_target: Option<BlockId>,
    },
    /// Try-catch construct
    TryBlock {
        try_block: BlockId,
        catch_block: Option<BlockId>,
        finally_block: Option<BlockId>,
    },
}

/// Expression representation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expression {
    /// Literal values
    Literal(Literal),
    /// Variable reference
    Variable(Variable),
    /// Binary operation
    BinaryOp {
        left: Box<Expression>,
        op: BinaryOperator,
        right: Box<Expression>,
    },
    /// Unary operation
    UnaryOp {
        op: UnaryOperator,
        operand: Box<Expression>,
    },
    /// Function call
    Call {
        function: String,
        arguments: Vec<Expression>,
    },
    /// Array/map access
    Index {
        array: Box<Expression>,
        index: Box<Expression>,
    },
    /// Field access
    Field {
        object: Box<Expression>,
        field: String,
    },
    /// Type conversion
    Cast {
        target_type: Type,
        expression: Box<Expression>,
    },
    /// Array construction
    Array(Vec<Expression>),
    /// Map construction
    Map(Vec<(Expression, Expression)>),
    /// Struct construction
    Struct { fields: Vec<(String, Expression)> },
    /// Array creation expression
    ArrayCreate {
        element_type: Box<Type>,
        size: Box<Expression>,
    },
    /// Struct creation expression
    StructCreate { fields: Vec<(String, Expression)> },
    /// Map creation expression
    MapCreate {
        entries: Vec<(Expression, Expression)>,
    },
}

/// Stack operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StackOp {
    Push,
    Pop,
    Dup,
    Swap,
    Drop,
    Pick,
    Roll,
    Reverse,
    Size,
    Clear,
}

/// Effect types for analysis
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectType {
    StorageRead,
    StorageWrite,
    EventEmit,
    ContractCall,
    Transfer,
    GasConsumption,
    RandomAccess,
    SystemStateRead,
    NetworkAccess,
    Pure,
}

/// Array operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArrayOperation {
    GetItem,
    SetItem,
    Append,
    Remove,
    Size,
    Clear,
}

/// Map operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MapOperation {
    Get,
    Set,
    HasKey,
    Remove,
    Keys,
    Values,
    Clear,
}

/// String operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StringOperation {
    Concat,
    Substring,
    Left,
    Right,
    Length,
}

/// Function parameter
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub param_type: Type,
    /// Parameter index
    pub index: u32,
}

/// Local variable
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalVariable {
    /// Variable name
    pub name: String,
    /// Variable type
    pub var_type: Type,
    /// Local variable type from type inference
    pub local_type: Type,
    /// Local slot index
    pub slot: u32,
    /// Is initialized
    pub initialized: bool,
}

/// Event definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventDefinition {
    /// Event name
    pub name: String,
    /// Event parameters
    pub parameters: Vec<Parameter>,
    /// Event description
    pub description: Option<String>,
}

/// Storage layout information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageLayout {
    /// Storage keys and their types
    pub keys: HashMap<String, Type>,
    /// Key patterns and descriptions
    pub patterns: Vec<KeyPattern>,
}

/// Function metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionMetadata {
    /// Original bytecode offset
    pub offset: Option<u32>,
    /// Is entry point
    pub is_entry_point: bool,
    /// Is exported/public
    pub is_public: bool,
    /// Is safe (read-only)
    pub is_safe: bool,
    /// Complexity metrics
    pub complexity: ComplexityMetrics,
}

/// Contract metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractMetadata {
    /// Contract name
    pub name: Option<String>,
    /// Compiler information
    pub compiler: Option<String>,
    /// Contract version
    pub version: Option<String>,
    /// Detected standards (NEP-17, etc.)
    pub standards: Vec<String>,
    /// Security analysis results
    pub security: SecurityMetadata,
}

/// Complexity metrics
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplexityMetrics {
    /// Cyclomatic complexity
    pub cyclomatic: u32,
    /// Number of basic blocks
    pub blocks: u32,
    /// Number of operations
    pub operations: u32,
    /// Maximum stack depth
    pub max_stack_depth: u32,
}

/// Security analysis metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityMetadata {
    /// Detected vulnerabilities
    pub vulnerabilities: Vec<Vulnerability>,
    /// Security score (0-100)
    pub score: u32,
    /// Risk level
    pub risk_level: RiskLevel,
}

/// Security vulnerability
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vulnerability {
    /// Vulnerability type
    pub vuln_type: String,
    /// Severity level
    pub severity: Severity,
    /// Description
    pub description: String,
    /// Location in code
    pub location: Option<CodeLocation>,
}

/// Risk levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Vulnerability severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

/// Code location for debugging
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeLocation {
    /// Function name
    pub function: String,
    /// Block ID
    pub block: BlockId,
    /// Operation index
    pub operation: usize,
    /// Bytecode offset
    pub offset: Option<u32>,
}

impl IRFunction {
    /// Create new IR function
    pub fn new(name: String) -> Self {
        Self {
            name,
            parameters: Vec::new(),
            locals: Vec::new(),
            blocks: HashMap::new(),
            entry_block: 0,
            exit_blocks: Vec::new(),
            return_type: None,
            metadata: FunctionMetadata::default(),
        }
    }

    /// Add basic block
    pub fn add_block(&mut self, block: IRBlock) {
        self.blocks.insert(block.id, block);
    }

    /// Get block by ID
    pub fn get_block(&self, id: BlockId) -> Option<&IRBlock> {
        self.blocks.get(&id)
    }

    /// Get mutable block by ID
    pub fn get_block_mut(&mut self, id: BlockId) -> Option<&mut IRBlock> {
        self.blocks.get_mut(&id)
    }

    /// Calculate complexity metrics
    pub fn calculate_complexity(&mut self) {
        let blocks = self.blocks.len() as u32;
        let operations = self
            .blocks
            .values()
            .map(|block| block.operations.len() as u32)
            .sum();

        // McCabe's cyclomatic complexity: edges - nodes + 2
        let edges = self
            .blocks
            .values()
            .map(|block| block.successors.len() as u32)
            .sum::<u32>();
        let cyclomatic = if edges >= blocks {
            edges - blocks + 2
        } else {
            1
        };

        self.metadata.complexity = ComplexityMetrics {
            cyclomatic,
            blocks,
            operations,
            max_stack_depth: 0, // Calculated during stack analysis pass
        };
    }
}

impl IRBlock {
    /// Create new basic block
    pub fn new(id: BlockId) -> Self {
        Self {
            id,
            operations: Vec::new(),
            terminator: Terminator::Return(None),
            predecessors: Vec::new(),
            successors: Vec::new(),
        }
    }

    /// Add operation to block
    pub fn add_operation(&mut self, operation: Operation) {
        self.operations.push(operation);
    }

    /// Set block terminator
    pub fn set_terminator(&mut self, terminator: Terminator) {
        self.terminator = terminator;
    }
}

impl Default for FunctionMetadata {
    fn default() -> Self {
        Self {
            offset: None,
            is_entry_point: false,
            is_public: false,
            is_safe: false,
            complexity: ComplexityMetrics {
                cyclomatic: 1,
                blocks: 0,
                operations: 0,
                max_stack_depth: 0,
            },
        }
    }
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "LOW"),
            RiskLevel::Medium => write!(f, "MEDIUM"),
            RiskLevel::High => write!(f, "HIGH"),
            RiskLevel::Critical => write!(f, "CRITICAL"),
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Info => write!(f, "INFO"),
            Severity::Low => write!(f, "LOW"),
            Severity::Medium => write!(f, "MEDIUM"),
            Severity::High => write!(f, "HIGH"),
            Severity::Critical => write!(f, "CRITICAL"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ir_function_creation() {
        let function = IRFunction::new("test_function".to_string());
        assert_eq!(function.name, "test_function");
        assert!(function.parameters.is_empty());
        assert!(function.blocks.is_empty());
    }

    #[test]
    fn test_ir_block_creation() {
        let block = IRBlock::new(0);
        assert_eq!(block.id, 0);
        assert!(block.operations.is_empty());
        assert!(matches!(block.terminator, Terminator::Return(None)));
    }

    #[test]
    fn test_add_block_to_function() {
        let mut function = IRFunction::new("test".to_string());
        let block = IRBlock::new(0);

        function.add_block(block);
        assert_eq!(function.blocks.len(), 1);
        assert!(function.get_block(0).is_some());
    }

    #[test]
    fn test_complexity_calculation() {
        let mut function = IRFunction::new("test".to_string());

        // Add two blocks with operations
        let mut block1 = IRBlock::new(0);
        block1.add_operation(Operation::Comment("test1".to_string()));
        block1.successors.push(1);

        let mut block2 = IRBlock::new(1);
        block2.add_operation(Operation::Comment("test2".to_string()));
        block2.add_operation(Operation::Comment("test3".to_string()));

        function.add_block(block1);
        function.add_block(block2);
        function.calculate_complexity();

        assert_eq!(function.metadata.complexity.blocks, 2);
        assert_eq!(function.metadata.complexity.operations, 3);
        assert_eq!(function.metadata.complexity.cyclomatic, 1); // 1 edge - 2 blocks + 2 = 1
    }
}
