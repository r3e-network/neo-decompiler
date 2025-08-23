//! Type system and type inference
//!
//! This module provides a sophisticated type inference engine for Neo N3 smart contracts,
//! implementing constraint-based type inference with support for Neo N3's complete type system.

use crate::common::types::*;
use crate::core::ir::{Expression, IRBlock, IRFunction, Operation};
use std::collections::HashMap;
use std::fmt;

/// Comprehensive type system for Neo N3 with enhanced type support
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Type {
    /// Neo N3 primitive types
    Primitive(PrimitiveType),
    /// Array types with element type
    Array(Box<Type>),
    /// Map types with key and value types
    Map { key: Box<Type>, value: Box<Type> },
    /// Buffer type (mutable byte array)
    Buffer,
    /// Struct types with named fields
    Struct(StructType),
    /// Union types for multiple possibilities
    Union(Vec<Type>),
    /// Function types with parameters and return type
    Function {
        parameters: Vec<Type>,
        return_type: Box<Type>,
    },
    /// Contract interface types
    Contract(ContractInterface),
    /// InteropInterface types for system interop
    InteropInterface(String),
    /// Pointer types
    Pointer(Box<Type>),
    /// Nullable types (type that can be null)
    Nullable(Box<Type>),
    /// Generic types with type parameters
    Generic { base: String, parameters: Vec<Type> },
    /// User-defined types
    UserDefined(String),
    /// Unknown/inferred type
    Unknown,
    /// Type variables for inference
    Variable(TypeVar),
    /// Any type (top type)
    Any,
    /// Never type (bottom type)
    Never,
    /// Void type (no value)
    Void,
}

/// Neo N3 primitive types with complete type hierarchy
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PrimitiveType {
    /// Boolean type (true/false)
    Boolean,
    /// Integer type (BigInteger)
    Integer,
    /// ByteString type (immutable byte array)
    ByteString,
    /// 160-bit hash (Address/Script Hash)
    Hash160,
    /// 256-bit hash (Transaction/Block Hash)
    Hash256,
    /// Elliptic curve point
    ECPoint,
    /// Public key
    PublicKey,
    /// Digital signature
    Signature,
    /// Null type
    Null,
    /// String type
    String,
    /// ByteArray type (mutable byte array)
    ByteArray,
}

/// Struct type definition with enhanced metadata
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct StructType {
    /// Struct name (optional)
    pub name: Option<String>,
    /// Struct fields
    pub fields: Vec<FieldType>,
    /// Is packed struct
    pub is_packed: bool,
}

/// Struct field definition with type and metadata
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FieldType {
    /// Field name
    pub name: String,
    /// Field type
    pub field_type: Type,
    /// Field index
    pub index: usize,
    /// Is field optional
    pub optional: bool,
}

/// Contract interface definition
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ContractInterface {
    /// Interface name
    pub name: String,
    /// Required methods
    pub methods: Vec<String>,
}

/// Comprehensive type inference context with advanced constraint solving
#[derive(Debug, Clone)]
pub struct TypeInferenceContext {
    /// Type constraints collected during analysis
    pub constraints: Vec<TypeConstraint>,
    /// Current type variable counter
    pub next_type_var: u32,
    /// Known type bindings from constraint solving
    pub bindings: HashMap<TypeVar, Type>,
    /// Function signatures from manifest/ABI
    pub function_types: HashMap<String, Type>,
    /// Syscall type signatures
    pub syscall_types: HashMap<u32, SyscallSignature>,
    /// Variable type assignments
    pub variable_types: HashMap<String, Type>,
    /// Storage key patterns and their types
    pub storage_types: HashMap<String, Type>,
    /// Type substitution chain
    pub substitutions: HashMap<TypeVar, TypeVar>,
    /// Type environments for scoped inference
    pub type_envs: Vec<TypeEnvironment>,
    /// Type error collection
    pub errors: Vec<TypeError>,
    /// Type inference statistics
    pub stats: InferenceStats,
}

/// Type environment for scoped type inference
#[derive(Debug, Clone, Default)]
pub struct TypeEnvironment {
    /// Variable types in current scope
    pub variables: HashMap<String, Type>,
    /// Local type definitions
    pub types: HashMap<String, Type>,
    /// Scope metadata
    pub scope_id: u32,
}

/// Syscall type signature information
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SyscallSignature {
    /// Syscall name
    pub name: String,
    /// Parameter types
    pub parameters: Vec<Type>,
    /// Return type
    pub return_type: Type,
    /// Side effects
    pub effects: Vec<SideEffect>,
}

/// Side effects of operations
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SideEffect {
    StorageRead,
    StorageWrite,
    ContractCall,
    EventEmit,
    StateChange,
    Pure,
}

/// Type inference statistics
#[derive(Debug, Clone, Default)]
pub struct InferenceStats {
    /// Number of constraints generated
    pub constraints_generated: usize,
    /// Number of constraints solved
    pub constraints_solved: usize,
    /// Number of type variables created
    pub type_vars_created: usize,
    /// Number of unification steps
    pub unification_steps: usize,
    /// Inference time in microseconds
    pub inference_time_us: u64,
}

/// Main type inference engine implementing constraint-based type inference
pub struct TypeInferenceEngine {
    /// Type inference context
    pub context: TypeInferenceContext,
}

/// Comprehensive type constraints for advanced inference
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TypeConstraint {
    /// Type equality constraint (t1 = t2)
    Equal(Type, Type),
    /// Subtype constraint (t1 <: t2)
    Subtype(Type, Type),
    /// Type must implement interface
    Implements(Type, ContractInterface),
    /// Type must support operation
    SupportsOperation(Type, BinaryOperator),
    /// Type must have field
    HasField(Type, String, Type),
    /// Type must be indexable
    Indexable(Type, Type, Type), // container[index] -> element
    /// Type must be callable
    Callable(Type, Vec<Type>, Type), // func(args) -> return
    /// Type must be convertible
    Convertible(Type, Type),
    /// Type must be nullable
    Nullable(Type),
    /// Type must be non-null
    NonNull(Type),
}

/// Type errors during inference
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TypeError {
    #[error("Type mismatch: expected {expected}, found {found}")]
    Mismatch { expected: Type, found: Type },

    #[error("Undefined variable: {name}")]
    UndefinedVariable { name: String },

    #[error("Type {type_name} does not support operation {operation}")]
    UnsupportedOperation {
        type_name: String,
        operation: String,
    },

    #[error("Cannot unify types {type1} and {type2}")]
    UnificationFailure { type1: Type, type2: Type },

    #[error("Infinite type detected in constraint")]
    InfiniteType,

    #[error("Constraint solving failed: {reason}")]
    ConstraintSolvingFailure { reason: String },

    #[error("Type {type_name} does not have field {field_name}")]
    FieldNotFound {
        type_name: String,
        field_name: String,
    },

    #[error("Cannot convert {from} to {to}")]
    ConversionError { from: Type, to: Type },
}

impl Type {
    /// Get a default value for this type or fallback to given type
    pub fn unwrap_or(self, fallback: Type) -> Type {
        match self {
            Type::Unknown => fallback,
            other => other,
        }
    }

    /// Check if type is numeric
    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::Primitive(PrimitiveType::Integer))
    }

    /// Check if type is boolean
    pub fn is_boolean(&self) -> bool {
        matches!(self, Type::Primitive(PrimitiveType::Boolean))
    }

    /// Check if type is a string type
    pub fn is_string(&self) -> bool {
        matches!(self, Type::Primitive(PrimitiveType::ByteString))
    }

    /// Check if type is nullable
    pub fn is_nullable(&self) -> bool {
        matches!(
            self,
            Type::Nullable(_) | Type::Primitive(PrimitiveType::Null)
        )
    }

    /// Check if type is a container (array, map, buffer)
    pub fn is_container(&self) -> bool {
        matches!(self, Type::Array(_) | Type::Map { .. } | Type::Buffer)
    }

    /// Check if type supports arithmetic operations
    pub fn supports_arithmetic(&self) -> bool {
        matches!(self, Type::Primitive(PrimitiveType::Integer))
    }

    /// Check if type supports comparison operations
    pub fn supports_comparison(&self) -> bool {
        matches!(
            self,
            Type::Primitive(
                PrimitiveType::Integer | PrimitiveType::Boolean | PrimitiveType::ByteString
            )
        )
    }

    /// Check if type supports indexing operations
    pub fn supports_indexing(&self) -> bool {
        matches!(
            self,
            Type::Array(_)
                | Type::Map { .. }
                | Type::Buffer
                | Type::Primitive(PrimitiveType::ByteString)
        )
    }

    /// Get the element type for container types
    pub fn element_type(&self) -> Option<&Type> {
        match self {
            Type::Array(element_type) => Some(element_type),
            Type::Map { value, .. } => Some(value),
            _ => None,
        }
    }

    /// Get the key type for map types
    pub fn key_type(&self) -> Option<&Type> {
        match self {
            Type::Map { key, .. } => Some(key),
            _ => None,
        }
    }

    /// Check if types are compatible (extended compatibility rules)
    pub fn is_compatible_with(&self, other: &Type) -> bool {
        match (self, other) {
            // Unknown types are compatible with everything
            (Type::Unknown, _) | (_, Type::Unknown) => true,
            (Type::Any, _) | (_, Type::Any) => true,

            // Exact type matches
            (a, b) if a == b => true,

            // Special conversions before general primitive matching
            (
                Type::Primitive(PrimitiveType::Integer),
                Type::Primitive(PrimitiveType::ByteString),
            ) => true,
            (
                Type::Primitive(PrimitiveType::ByteString),
                Type::Primitive(PrimitiveType::Integer),
            ) => true,

            // Primitive type compatibility
            (Type::Primitive(a), Type::Primitive(b)) => a == b,

            // Array compatibility (covariant in element type)
            (Type::Array(a), Type::Array(b)) => a.is_compatible_with(b),

            // Map compatibility (covariant in both key and value)
            (Type::Map { key: k1, value: v1 }, Type::Map { key: k2, value: v2 }) => {
                k1.is_compatible_with(k2) && v1.is_compatible_with(v2)
            }

            // Nullable compatibility
            (Type::Nullable(inner), other) => inner.is_compatible_with(other),
            (other, Type::Nullable(inner)) => other.is_compatible_with(inner),

            // Union type compatibility
            (Type::Union(types), other) => types.iter().any(|t| t.is_compatible_with(other)),
            (other, Type::Union(types)) => types.iter().any(|t| other.is_compatible_with(t)),

            // Buffer and ByteString are somewhat compatible
            (Type::Buffer, Type::Primitive(PrimitiveType::ByteString)) => true,
            (Type::Primitive(PrimitiveType::ByteString), Type::Buffer) => true,

            _ => false,
        }
    }

    /// Check if this type can be assigned to another type
    pub fn is_assignable_to(&self, target: &Type) -> bool {
        self.is_subtype_of(target)
    }

    /// Check if this type is a subtype of another type
    pub fn is_subtype_of(&self, supertype: &Type) -> bool {
        match (self, supertype) {
            // Any type is a subtype of Any
            (_, Type::Any) => true,

            // Never is a subtype of any type
            (Type::Never, _) => true,

            // Null is a subtype of nullable types
            (Type::Primitive(PrimitiveType::Null), Type::Nullable(_)) => true,

            // Type variables and unknown types
            (Type::Variable(_), _) | (_, Type::Variable(_)) => true,
            (Type::Unknown, _) | (_, Type::Unknown) => true,

            // Exact matches
            (a, b) if a == b => true,

            // Structural subtyping for arrays (covariant)
            (Type::Array(elem1), Type::Array(elem2)) => elem1.is_subtype_of(elem2),

            // Structural subtyping for maps
            (Type::Map { key: k1, value: v1 }, Type::Map { key: k2, value: v2 }) => {
                k1.is_subtype_of(k2) && v1.is_subtype_of(v2)
            }

            // Union types
            (Type::Union(types), target) => types.iter().all(|t| t.is_subtype_of(target)),
            (source, Type::Union(types)) => types.iter().any(|t| source.is_subtype_of(t)),

            _ => false,
        }
    }

    /// Get the size in bytes for all Neo N3 types (complete implementation)
    pub fn byte_size(&self) -> Option<usize> {
        match self {
            // Fixed-size primitive types
            Type::Primitive(PrimitiveType::Boolean) => Some(1),
            Type::Primitive(PrimitiveType::Hash160) => Some(20),
            Type::Primitive(PrimitiveType::Hash256) => Some(32),
            Type::Primitive(PrimitiveType::ECPoint) => Some(33), // Compressed point
            Type::Primitive(PrimitiveType::Signature) => Some(64), // ECDSA signature
            Type::Primitive(PrimitiveType::PublicKey) => Some(33), // Compressed public key
            Type::Primitive(PrimitiveType::Null) => Some(1),     // Null marker

            // Variable-size types return None
            Type::Primitive(PrimitiveType::Integer) => None, // BigInteger - variable size
            Type::Primitive(PrimitiveType::ByteString) => None, // Variable length
            Type::Primitive(PrimitiveType::String) => None,  // Variable length string
            Type::Primitive(PrimitiveType::ByteArray) => None, // Variable length byte array
            Type::Array(_) => None,                          // Variable number of elements
            Type::Map { .. } => None,                        // Variable number of key-value pairs
            Type::Buffer => None,                            // Variable size mutable buffer
            Type::Struct(_) => None,                         // Depends on field types
            Type::Union(_) => None,                          // Depends on active type
            Type::Function { .. } => None,                   // Functions don't have a fixed size
            Type::Contract(_) => Some(20),                   // Contract hash is 160-bit
            Type::InteropInterface(_) => None,               // Opaque type
            Type::Pointer(_) => Some(8),                     // 64-bit pointer
            Type::Nullable(inner) => {
                // Nullable adds overhead for null flag
                inner.byte_size().map(|size| size + 1)
            }
            Type::Generic { .. } => None, // Depends on instantiation
            Type::UserDefined(_) => None, // Unknown size
            Type::Unknown | Type::Variable(_) | Type::Any | Type::Never => None,
            Type::Void => Some(0), // Void has no size
        }
    }

    /// Check if type is a primitive Neo N3 type
    pub fn is_primitive(&self) -> bool {
        matches!(self, Type::Primitive(_))
    }

    /// Check if type is a composite type (struct, array, map)
    pub fn is_composite(&self) -> bool {
        matches!(self, Type::Struct(_) | Type::Array(_) | Type::Map { .. })
    }

    /// Check if type supports serialization to ByteString
    pub fn is_serializable(&self) -> bool {
        match self {
            Type::Primitive(_) => true,
            Type::Array(_) | Type::Map { .. } | Type::Struct(_) => true,
            Type::Buffer => true,
            Type::Contract(_) => true,
            Type::Nullable(inner) => inner.is_serializable(),
            Type::Union(types) => types.iter().all(|t| t.is_serializable()),
            _ => false,
        }
    }

    /// Get the Neo N3 stack item type for this type
    pub fn to_stack_item_type(&self) -> StackItemType {
        match self {
            Type::Primitive(PrimitiveType::Boolean) => StackItemType::Boolean,
            Type::Primitive(PrimitiveType::Integer) => StackItemType::Integer,
            Type::Primitive(PrimitiveType::ByteString) => StackItemType::ByteString,
            Type::Primitive(PrimitiveType::Hash160) => StackItemType::ByteString,
            Type::Primitive(PrimitiveType::Hash256) => StackItemType::ByteString,
            Type::Primitive(PrimitiveType::ECPoint) => StackItemType::ByteString,
            Type::Primitive(PrimitiveType::PublicKey) => StackItemType::ByteString,
            Type::Primitive(PrimitiveType::Signature) => StackItemType::ByteString,
            Type::Primitive(PrimitiveType::Null) => StackItemType::Any,
            Type::Array(_) => StackItemType::Array,
            Type::Map { .. } => StackItemType::Map,
            Type::Buffer => StackItemType::Buffer,
            Type::Struct(_) => StackItemType::Struct,
            Type::InteropInterface(_) => StackItemType::InteropInterface,
            Type::Pointer(_) => StackItemType::Pointer,
            _ => StackItemType::Any,
        }
    }

    /// Check if this type can be used in a conditional context
    pub fn is_truthy_type(&self) -> bool {
        match self {
            Type::Primitive(PrimitiveType::Boolean) => true,
            Type::Primitive(PrimitiveType::Integer) => true, // 0 is false, others true
            Type::Primitive(PrimitiveType::ByteString) => true, // Empty string is false
            Type::Array(_) => true,                          // Empty array is false
            Type::Map { .. } => true,                        // Empty map is false
            Type::Buffer => true,                            // Empty buffer is false
            Type::Primitive(PrimitiveType::Null) => false,   // Always false
            Type::Nullable(_) => true, // Null is false, value follows inner type
            _ => false,
        }
    }

    /// Convert type to string representation
    pub fn to_string(&self) -> String {
        match self {
            Type::Primitive(p) => format!("{:?}", p),
            Type::Array(elem) => format!("Array<{}>", elem.to_string()),
            Type::Map { key, value } => format!("Map<{}, {}>", key.to_string(), value.to_string()),
            Type::Buffer => "Buffer".to_string(),
            Type::Struct(s) => s.name.clone().unwrap_or_else(|| "Struct".to_string()),
            Type::Union(types) => {
                let type_strs: Vec<String> = types.iter().map(|t| t.to_string()).collect();
                format!("({})", type_strs.join(" | "))
            }
            Type::Function {
                parameters,
                return_type,
            } => {
                let param_strs: Vec<String> = parameters.iter().map(|p| p.to_string()).collect();
                format!("({}) -> {}", param_strs.join(", "), return_type.to_string())
            }
            Type::Nullable(inner) => format!("{}?", inner.to_string()),
            Type::Generic { base, parameters } => {
                let param_strs: Vec<String> = parameters.iter().map(|p| p.to_string()).collect();
                format!("{}<{}>", base, param_strs.join(", "))
            }
            Type::Contract(c) => c.name.clone(),
            Type::InteropInterface(name) => format!("InteropInterface({})", name),
            Type::Pointer(inner) => format!("*{}", inner.to_string()),
            Type::UserDefined(name) => name.clone(),
            Type::Variable(var) => format!("T{}", var),
            Type::Unknown => "?".to_string(),
            Type::Any => "Any".to_string(),
            Type::Never => "Never".to_string(),
            Type::Void => "void".to_string(),
        }
    }

    /// Create type for Neo N3 array with specific element type
    pub fn array(element_type: Type) -> Type {
        Type::Array(Box::new(element_type))
    }

    /// Create type for Neo N3 map with specific key and value types
    pub fn map(key_type: Type, value_type: Type) -> Type {
        Type::Map {
            key: Box::new(key_type),
            value: Box::new(value_type),
        }
    }

    /// Create nullable type
    pub fn nullable(inner_type: Type) -> Type {
        Type::Nullable(Box::new(inner_type))
    }

    /// Create function type
    pub fn function(parameters: Vec<Type>, return_type: Type) -> Type {
        Type::Function {
            parameters,
            return_type: Box::new(return_type),
        }
    }
}

impl TypeInferenceContext {
    /// Create new type inference context
    pub fn new() -> Self {
        Self {
            constraints: Vec::new(),
            next_type_var: 0,
            bindings: HashMap::new(),
            function_types: HashMap::new(),
            syscall_types: HashMap::new(),
            variable_types: HashMap::new(),
            storage_types: HashMap::new(),
            substitutions: HashMap::new(),
            type_envs: vec![TypeEnvironment::default()],
            errors: Vec::new(),
            stats: InferenceStats::default(),
        }
    }

    /// Create fresh type variable
    pub fn fresh_type_var(&mut self) -> Type {
        let var = self.next_type_var;
        self.next_type_var += 1;
        self.stats.type_vars_created += 1;
        Type::Variable(var)
    }

    /// Add type constraint
    pub fn add_constraint(&mut self, constraint: TypeConstraint) {
        self.constraints.push(constraint);
        self.stats.constraints_generated += 1;
    }

    /// Get type of variable
    pub fn get_variable_type(&self, name: &str) -> Option<&Type> {
        // Check current scope first
        if let Some(env) = self.type_envs.last() {
            if let Some(ty) = env.variables.get(name) {
                return Some(ty);
            }
        }

        // Check global variable types
        self.variable_types.get(name)
    }

    /// Set variable type in current scope
    pub fn set_variable_type(&mut self, name: String, ty: Type) {
        if let Some(env) = self.type_envs.last_mut() {
            env.variables.insert(name.clone(), ty.clone());
        }
        self.variable_types.insert(name, ty);
    }

    /// Push new type environment scope
    pub fn push_scope(&mut self) {
        let scope_id = self.type_envs.len() as u32;
        self.type_envs.push(TypeEnvironment {
            variables: HashMap::new(),
            types: HashMap::new(),
            scope_id,
        });
    }

    /// Pop current type environment scope
    pub fn pop_scope(&mut self) {
        if self.type_envs.len() > 1 {
            self.type_envs.pop();
        }
    }

    /// Add type error
    pub fn add_error(&mut self, error: TypeError) {
        self.errors.push(error);
    }

    /// Check if there are type errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Get all type errors
    pub fn get_errors(&self) -> &[TypeError] {
        &self.errors
    }

    /// Get syscall type signature
    pub fn get_syscall_type(&self, hash: u32) -> Option<&SyscallSignature> {
        self.syscall_types.get(&hash)
    }
}

impl TypeInferenceEngine {
    /// Create new type inference engine
    pub fn new() -> Self {
        let mut context = TypeInferenceContext::new();

        // Initialize with comprehensive Neo N3 syscalls
        Self::init_syscall_database(&mut context);

        Self { context }
    }

    /// Initialize complete syscall type database
    fn init_syscall_database(context: &mut TypeInferenceContext) {
        // Storage syscalls
        context.syscall_types.insert(
            0x0c40166b,
            SyscallSignature {
                name: "System.Storage.Get".to_string(),
                parameters: vec![Type::Primitive(PrimitiveType::ByteString)],
                return_type: Type::Nullable(Box::new(Type::Primitive(PrimitiveType::ByteString))),
                effects: vec![SideEffect::StorageRead],
            },
        );

        context.syscall_types.insert(
            0xd90f55cf,
            SyscallSignature {
                name: "System.Storage.Put".to_string(),
                parameters: vec![
                    Type::Primitive(PrimitiveType::ByteString),
                    Type::Primitive(PrimitiveType::ByteString),
                ],
                return_type: Type::Primitive(PrimitiveType::Null),
                effects: vec![SideEffect::StorageWrite],
            },
        );

        context.syscall_types.insert(
            0x64122a5c,
            SyscallSignature {
                name: "System.Storage.Delete".to_string(),
                parameters: vec![Type::Primitive(PrimitiveType::ByteString)],
                return_type: Type::Primitive(PrimitiveType::Null),
                effects: vec![SideEffect::StorageWrite],
            },
        );

        // Blockchain syscalls
        context.syscall_types.insert(
            0xdde70a99,
            SyscallSignature {
                name: "System.Blockchain.GetHeight".to_string(),
                parameters: vec![],
                return_type: Type::Primitive(PrimitiveType::Integer),
                effects: vec![SideEffect::Pure],
            },
        );

        // Contract syscalls
        context.syscall_types.insert(
            0x627d5b52,
            SyscallSignature {
                name: "System.Contract.Call".to_string(),
                parameters: vec![
                    Type::Primitive(PrimitiveType::Hash160),
                    Type::Primitive(PrimitiveType::ByteString),
                    Type::Array(Box::new(Type::Any)),
                ],
                return_type: Type::Any,
                effects: vec![SideEffect::ContractCall],
            },
        );

        // Runtime syscalls
        context.syscall_types.insert(
            0x49aa75e6,
            SyscallSignature {
                name: "System.Runtime.Log".to_string(),
                parameters: vec![Type::Primitive(PrimitiveType::ByteString)],
                return_type: Type::Primitive(PrimitiveType::Null),
                effects: vec![SideEffect::EventEmit],
            },
        );

        // Crypto syscalls
        context.syscall_types.insert(
            0x16ed57a1,
            SyscallSignature {
                name: "System.Crypto.CheckSig".to_string(),
                parameters: vec![
                    Type::Primitive(PrimitiveType::Signature),
                    Type::Primitive(PrimitiveType::ECPoint),
                ],
                return_type: Type::Primitive(PrimitiveType::Boolean),
                effects: vec![SideEffect::Pure],
            },
        );
    }

    /// Main type inference entry point
    pub fn infer_types(&mut self, function: &mut IRFunction) -> Result<(), TypeError> {
        let start_time = std::time::Instant::now();

        // Phase 1: Collect type constraints from all blocks
        for (_block_id, block) in &function.blocks {
            self.collect_block_constraints(block)?;
        }

        // Phase 2: Add parameter and local variable constraints
        self.collect_function_constraints(function)?;

        // Phase 3: Solve all collected constraints
        self.solve_constraints()?;

        // Phase 4: Apply inferred types back to function
        self.apply_inferred_types(function)?;

        // Update statistics
        self.context.stats.inference_time_us = start_time.elapsed().as_micros() as u64;

        if self.context.has_errors() {
            Err(self.context.errors[0].clone())
        } else {
            Ok(())
        }
    }

    /// Collect type constraints from a basic block
    fn collect_block_constraints(&mut self, block: &IRBlock) -> Result<(), TypeError> {
        for operation in &block.operations {
            self.collect_operation_constraints(operation)?;
        }
        Ok(())
    }

    /// Collect constraints from function parameters and locals
    fn collect_function_constraints(&mut self, function: &IRFunction) -> Result<(), TypeError> {
        // Add parameter type constraints
        for param in &function.parameters {
            if param.param_type != Type::Unknown {
                self.context
                    .set_variable_type(param.name.clone(), param.param_type.clone());
            }
        }

        // Add local variable constraints
        for local in &function.locals {
            if local.var_type != Type::Unknown {
                self.context
                    .set_variable_type(local.name.clone(), local.var_type.clone());
            }
            // Also consider the inferred type from local_type field
            if local.local_type != Type::Unknown {
                let var_type = self
                    .context
                    .get_variable_type(&local.name)
                    .cloned()
                    .unwrap_or(Type::Unknown);
                self.context
                    .add_constraint(TypeConstraint::Equal(var_type, local.local_type.clone()));
            }
        }

        Ok(())
    }

    /// Collect type constraints from an operation
    fn collect_operation_constraints(&mut self, operation: &Operation) -> Result<(), TypeError> {
        use crate::core::ir::Operation;

        match operation {
            Operation::Assign { target, source } => {
                let source_type = self.infer_expression_type(source)?;
                let target_type = Type::Variable({
                    let var_id = self.context.next_type_var;
                    self.context.next_type_var += 1;
                    var_id
                });

                self.context
                    .add_constraint(TypeConstraint::Equal(target_type.clone(), source_type));
                self.context
                    .set_variable_type(target.name.clone(), target_type);
            }

            Operation::Arithmetic {
                target,
                left,
                right,
                operator,
            } => {
                let left_type = self.infer_expression_type(left)?;
                let right_type = self.infer_expression_type(right)?;
                let result_type =
                    self.infer_binary_result_type(&left_type, &right_type, *operator)?;

                self.context
                    .add_constraint(TypeConstraint::SupportsOperation(
                        left_type.clone(),
                        *operator,
                    ));
                self.context
                    .add_constraint(TypeConstraint::SupportsOperation(
                        right_type.clone(),
                        *operator,
                    ));
                self.context
                    .set_variable_type(target.name.clone(), result_type);
            }

            Operation::Unary {
                target,
                operand,
                operator,
            } => {
                let operand_type = self.infer_expression_type(operand)?;
                let result_type = self.infer_unary_result_type(&operand_type, *operator)?;

                self.context
                    .set_variable_type(target.name.clone(), result_type);
            }

            Operation::Syscall {
                name: _,
                arguments,
                return_type,
                target,
            } => {
                // Handle syscall invocations
                if let Some(result_var) = target {
                    let result_type = return_type.clone().unwrap_or(Type::Unknown);
                    self.context
                        .set_variable_type(result_var.name.clone(), result_type);
                }

                // Add constraints for argument types
                for arg in arguments {
                    let _arg_type = self.infer_expression_type(arg)?;
                    // Note: syscall argument validation could be added here
                }
            }

            // Handle other operation types that don't affect type constraints directly
            Operation::ContractCall { target, .. } => {
                // Contract call result type - could be inferred from contract ABI
                if let Some(result_var) = target {
                    self.context
                        .set_variable_type(result_var.name.clone(), Type::Unknown);
                }
            }

            Operation::Storage { target, .. } => {
                // Storage operations typically return the stored/loaded value
                if let Some(result_var) = target {
                    self.context
                        .set_variable_type(result_var.name.clone(), Type::Unknown);
                }
            }

            _ => {
                // Handle other operation types as needed
            }
        }

        Ok(())
    }

    /// Solve all collected type constraints using unification
    pub fn solve_constraints(&mut self) -> Result<(), TypeError> {
        let mut changed = true;
        let mut iteration = 0;
        const MAX_ITERATIONS: usize = 100;

        while changed && iteration < MAX_ITERATIONS {
            changed = false;
            iteration += 1;

            // Process each constraint
            for constraint in self.context.constraints.clone() {
                if self.solve_single_constraint(&constraint)? {
                    changed = true;
                }
            }

            self.context.stats.unification_steps += 1;
        }

        if iteration >= MAX_ITERATIONS {
            return Err(TypeError::ConstraintSolvingFailure {
                reason: "Maximum iterations exceeded in constraint solving".to_string(),
            });
        }

        self.context.stats.constraints_solved = self.context.constraints.len();
        Ok(())
    }

    /// Solve a single type constraint
    fn solve_single_constraint(&mut self, constraint: &TypeConstraint) -> Result<bool, TypeError> {
        match constraint {
            TypeConstraint::Equal(t1, t2) => self.unify(t1, t2),

            TypeConstraint::Subtype(sub, sup) => {
                // Check if subtype relationship already holds
                let resolved_sub = self.resolve_type(sub);
                let resolved_sup = self.resolve_type(sup);

                if !resolved_sub.is_subtype_of(&resolved_sup) {
                    // Try to make it work through unification
                    self.unify(&resolved_sub, &resolved_sup)
                } else {
                    Ok(false) // Already satisfied
                }
            }

            TypeConstraint::SupportsOperation(t, op) => {
                let resolved_type = self.resolve_type(t);

                if !self.type_supports_operation(&resolved_type, *op) {
                    // Try to find a compatible type
                    let compatible_type =
                        self.find_operation_compatible_type(&resolved_type, *op)?;
                    self.unify(&resolved_type, &compatible_type)
                } else {
                    Ok(false) // Already satisfied
                }
            }

            TypeConstraint::HasField(t, field_name, field_type) => {
                let resolved_type = self.resolve_type(t);

                match resolved_type {
                    Type::Struct(ref struct_type) => {
                        if let Some(field) =
                            struct_type.fields.iter().find(|f| f.name == *field_name)
                        {
                            self.unify(&field.field_type, field_type)
                        } else {
                            Err(TypeError::FieldNotFound {
                                type_name: struct_type
                                    .name
                                    .clone()
                                    .unwrap_or("Anonymous".to_string()),
                                field_name: field_name.clone(),
                            })
                        }
                    }
                    Type::Variable(_) => {
                        // Create struct type with this field
                        let struct_type = Type::Struct(StructType {
                            name: None,
                            fields: vec![FieldType {
                                name: field_name.clone(),
                                field_type: field_type.clone(),
                                index: 0,
                                optional: false,
                            }],
                            is_packed: false,
                        });
                        self.unify(&resolved_type, &struct_type)
                    }
                    _ => Err(TypeError::FieldNotFound {
                        type_name: resolved_type.to_string(),
                        field_name: field_name.clone(),
                    }),
                }
            }

            TypeConstraint::Indexable(container, index, element) => {
                let resolved_container = self.resolve_type(container);
                let resolved_index = self.resolve_type(index);
                let resolved_element = self.resolve_type(element);

                match resolved_container {
                    Type::Array(ref elem_type) => {
                        // Index must be integer, element must match array element type
                        let index_unified =
                            self.unify(&resolved_index, &Type::Primitive(PrimitiveType::Integer))?;
                        let element_unified = self.unify(&resolved_element, elem_type)?;
                        Ok(index_unified || element_unified)
                    }
                    Type::Map { ref key, ref value } => {
                        // Index must match key type, element must match value type
                        let key_unified = self.unify(&resolved_index, key)?;
                        let value_unified = self.unify(&resolved_element, value)?;
                        Ok(key_unified || value_unified)
                    }
                    Type::Variable(_) => {
                        // Create appropriate container type
                        let container_type =
                            if resolved_index == Type::Primitive(PrimitiveType::Integer) {
                                Type::Array(Box::new(resolved_element.clone()))
                            } else {
                                Type::Map {
                                    key: Box::new(resolved_index.clone()),
                                    value: Box::new(resolved_element.clone()),
                                }
                            };
                        self.unify(&resolved_container, &container_type)
                    }
                    _ => Err(TypeError::UnsupportedOperation {
                        type_name: resolved_container.to_string(),
                        operation: "indexing".to_string(),
                    }),
                }
            }

            TypeConstraint::Convertible(from, to) => {
                let resolved_from = self.resolve_type(from);
                let resolved_to = self.resolve_type(to);

                if self.is_convertible(&resolved_from, &resolved_to) {
                    Ok(false) // Already satisfied
                } else {
                    Err(TypeError::ConversionError {
                        from: resolved_from,
                        to: resolved_to,
                    })
                }
            }

            _ => Ok(false), // Other constraints not yet implemented
        }
    }

    /// Unify two types with occurs check
    fn unify(&mut self, t1: &Type, t2: &Type) -> Result<bool, TypeError> {
        let resolved1 = self.resolve_type(t1);
        let resolved2 = self.resolve_type(t2);

        if resolved1 == resolved2 {
            return Ok(false); // Already unified
        }

        match (&resolved1, &resolved2) {
            // Variable unification
            (Type::Variable(var1), Type::Variable(var2)) => {
                if var1 != var2 {
                    self.context.bindings.insert(*var1, resolved2.clone());
                    Ok(true)
                } else {
                    Ok(false)
                }
            }

            (Type::Variable(var), other) | (other, Type::Variable(var)) => {
                // Occurs check
                if self.occurs_check(*var, other) {
                    return Err(TypeError::InfiniteType);
                }

                self.context.bindings.insert(*var, other.clone());
                Ok(true)
            }

            // Structural unification
            (Type::Array(elem1), Type::Array(elem2)) => self.unify(elem1, elem2),

            (Type::Map { key: k1, value: v1 }, Type::Map { key: k2, value: v2 }) => {
                let key_unified = self.unify(k1, k2)?;
                let value_unified = self.unify(v1, v2)?;
                Ok(key_unified || value_unified)
            }

            (Type::Nullable(inner1), Type::Nullable(inner2)) => self.unify(inner1, inner2),

            // Compatible types that can be unified
            (t1, t2) if t1.is_compatible_with(t2) => {
                // Choose the more specific type
                let unified_type = self.choose_more_specific_type(t1, t2);
                if unified_type != resolved1 {
                    if let Type::Variable(var) = &resolved1 {
                        self.context.bindings.insert(*var, unified_type);
                        return Ok(true);
                    }
                }
                if unified_type != resolved2 {
                    if let Type::Variable(var) = &resolved2 {
                        self.context.bindings.insert(*var, unified_type);
                        return Ok(true);
                    }
                }
                Ok(false)
            }

            // Unification failure
            _ => Err(TypeError::UnificationFailure {
                type1: resolved1,
                type2: resolved2,
            }),
        }
    }

    /// Occurs check to prevent infinite types
    fn occurs_check(&self, var: TypeVar, ty: &Type) -> bool {
        match ty {
            Type::Variable(other_var) => {
                if var == *other_var {
                    true
                } else if let Some(bound_type) = self.context.bindings.get(other_var) {
                    self.occurs_check(var, bound_type)
                } else {
                    false
                }
            }
            Type::Array(elem) => self.occurs_check(var, elem),
            Type::Map { key, value } => {
                self.occurs_check(var, key) || self.occurs_check(var, value)
            }
            Type::Nullable(inner) => self.occurs_check(var, inner),
            Type::Function {
                parameters,
                return_type,
            } => {
                parameters.iter().any(|p| self.occurs_check(var, p))
                    || self.occurs_check(var, return_type)
            }
            Type::Generic { parameters, .. } => {
                parameters.iter().any(|p| self.occurs_check(var, p))
            }
            Type::Struct(struct_type) => struct_type
                .fields
                .iter()
                .any(|f| self.occurs_check(var, &f.field_type)),
            _ => false,
        }
    }

    /// Resolve type by following variable bindings
    fn resolve_type(&self, ty: &Type) -> Type {
        match ty {
            Type::Variable(var) => {
                if let Some(bound_type) = self.context.bindings.get(var) {
                    self.resolve_type(bound_type)
                } else {
                    ty.clone()
                }
            }
            _ => ty.clone(),
        }
    }

    /// Choose the more specific of two compatible types
    fn choose_more_specific_type(&self, t1: &Type, t2: &Type) -> Type {
        match (t1, t2) {
            (Type::Unknown, other) | (other, Type::Unknown) => other.clone(),
            (Type::Any, other) | (other, Type::Any) => other.clone(),
            (Type::Variable(_), other) | (other, Type::Variable(_)) => other.clone(),
            _ => {
                // If both are concrete, prefer the subtype
                if t1.is_subtype_of(t2) {
                    t1.clone()
                } else if t2.is_subtype_of(t1) {
                    t2.clone()
                } else {
                    t1.clone() // Arbitrary choice
                }
            }
        }
    }

    /// Get common supertype of two types
    pub fn common_supertype(&self, t1: &Type, t2: &Type) -> Type {
        let resolved1 = self.resolve_type(t1);
        let resolved2 = self.resolve_type(t2);

        match (&resolved1, &resolved2) {
            (a, b) if a == b => a.clone(),

            // Special Neo N3 type compatibility rules
            (Type::Primitive(PrimitiveType::Integer), Type::Primitive(PrimitiveType::Boolean))
            | (Type::Primitive(PrimitiveType::Boolean), Type::Primitive(PrimitiveType::Integer)) => {
                Type::Any // No common supertype, must be Any
            }

            (
                Type::Primitive(PrimitiveType::Integer),
                Type::Primitive(PrimitiveType::ByteString),
            )
            | (
                Type::Primitive(PrimitiveType::ByteString),
                Type::Primitive(PrimitiveType::Integer),
            ) => {
                Type::Any // Both are convertible but no direct supertype
            }

            // Array types - covariant in element type
            (Type::Array(elem1), Type::Array(elem2)) => {
                Type::Array(Box::new(self.common_supertype(elem1, elem2)))
            }

            // Map types
            (Type::Map { key: k1, value: v1 }, Type::Map { key: k2, value: v2 }) => Type::Map {
                key: Box::new(self.common_supertype(k1, k2)),
                value: Box::new(self.common_supertype(v1, v2)),
            },

            // Nullable types
            (Type::Nullable(inner1), Type::Nullable(inner2)) => {
                Type::Nullable(Box::new(self.common_supertype(inner1, inner2)))
            }

            (Type::Nullable(inner), other) | (other, Type::Nullable(inner)) => {
                Type::Nullable(Box::new(self.common_supertype(inner, other)))
            }

            // Union types
            (Type::Union(types1), Type::Union(types2)) => {
                let mut all_types = types1.clone();
                all_types.extend(types2.clone());
                all_types.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
                all_types.dedup();
                Type::Union(all_types)
            }

            (Type::Union(types), other) | (other, Type::Union(types)) => {
                let mut all_types = types.clone();
                all_types.push(other.clone());
                all_types.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
                all_types.dedup();
                Type::Union(all_types)
            }

            // Default to Any for incompatible types
            _ => Type::Any,
        }
    }

    /// Unify multiple types to find common type
    fn unify_types(&mut self, types: Vec<Type>) -> Result<Type, TypeError> {
        if types.is_empty() {
            return Ok(Type::Unknown);
        }

        if types.len() == 1 {
            return Ok(types[0].clone());
        }

        // Start with first type and unify with each subsequent type
        let mut result = types[0].clone();
        for ty in types.iter().skip(1) {
            result = self.common_supertype(&result, ty);
        }

        Ok(result)
    }

    /// Infer type of an expression
    pub fn infer_expression_type(&mut self, expr: &Expression) -> Result<Type, TypeError> {
        match expr {
            Expression::Variable(var) => self
                .context
                .get_variable_type(&var.name)
                .cloned()
                .ok_or_else(|| TypeError::UndefinedVariable {
                    name: var.name.clone(),
                }),

            Expression::Literal(literal) => Ok(self.infer_literal_type(literal)),

            Expression::BinaryOp { left, right, op } => {
                let left_type = self.infer_expression_type(left)?;
                let right_type = self.infer_expression_type(right)?;
                self.infer_binary_result_type(&left_type, &right_type, *op)
            }

            Expression::UnaryOp { operand, op } => {
                let operand_type = self.infer_expression_type(operand)?;
                self.infer_unary_result_type(&operand_type, *op)
            }

            Expression::Call {
                function,
                arguments,
            } => self.infer_call_result_type(function, arguments),

            Expression::Field { object, field } => {
                let object_type = self.infer_expression_type(object)?;
                self.infer_field_access_type(&object_type, field)
            }

            Expression::Index { array, index } => {
                let array_type = self.infer_expression_type(array)?;
                let index_type = self.infer_expression_type(index)?;
                self.infer_index_result_type(&array_type, &index_type)
            }

            Expression::Cast {
                target_type,
                expression,
            } => {
                let source_type = self.infer_expression_type(expression)?;
                if self.is_convertible(&source_type, target_type) {
                    Ok(target_type.clone())
                } else {
                    Err(TypeError::ConversionError {
                        from: source_type,
                        to: target_type.clone(),
                    })
                }
            }

            _ => Ok(Type::Unknown), // Fallback for unhandled expressions
        }
    }

    /// Infer type from literal value
    fn infer_literal_type(&self, literal: &Literal) -> Type {
        match literal {
            Literal::Boolean(_) => Type::Primitive(PrimitiveType::Boolean),
            Literal::Integer(_) => Type::Primitive(PrimitiveType::Integer),
            Literal::BigInteger(_) => Type::Primitive(PrimitiveType::Integer),
            Literal::String(_) => Type::Primitive(PrimitiveType::ByteString),
            Literal::ByteArray(_) => Type::Primitive(PrimitiveType::ByteString),
            Literal::Hash160(_) => Type::Primitive(PrimitiveType::Hash160),
            Literal::Hash256(_) => Type::Primitive(PrimitiveType::Hash256),
            Literal::Null => Type::Primitive(PrimitiveType::Null),
        }
    }

    /// Infer result type of binary operation
    fn infer_binary_result_type(
        &self,
        left: &Type,
        right: &Type,
        op: BinaryOperator,
    ) -> Result<Type, TypeError> {
        use BinaryOperator::*;

        let left_resolved = self.resolve_type(left);
        let right_resolved = self.resolve_type(right);

        match op {
            // Arithmetic operations - require numeric types
            Add | Sub | Mul | Div | Mod | Pow => {
                match (&left_resolved, &right_resolved) {
                    (
                        Type::Primitive(PrimitiveType::Integer),
                        Type::Primitive(PrimitiveType::Integer),
                    ) => Ok(Type::Primitive(PrimitiveType::Integer)),
                    // String concatenation for ADD
                    (
                        Type::Primitive(PrimitiveType::ByteString),
                        Type::Primitive(PrimitiveType::ByteString),
                    ) if op == Add => Ok(Type::Primitive(PrimitiveType::ByteString)),
                    _ => Err(TypeError::UnsupportedOperation {
                        type_name: format!(
                            "{} {} {}",
                            left_resolved.to_string(),
                            format!("{:?}", op),
                            right_resolved.to_string()
                        ),
                        operation: format!("{:?}", op),
                    }),
                }
            }

            // Bitwise operations - require integer types
            And | Or | Xor | LeftShift | RightShift => match (&left_resolved, &right_resolved) {
                (
                    Type::Primitive(PrimitiveType::Integer),
                    Type::Primitive(PrimitiveType::Integer),
                ) => Ok(Type::Primitive(PrimitiveType::Integer)),
                _ => Err(TypeError::UnsupportedOperation {
                    type_name: format!(
                        "{} {} {}",
                        left_resolved.to_string(),
                        format!("{:?}", op),
                        right_resolved.to_string()
                    ),
                    operation: format!("{:?}", op),
                }),
            },

            // Comparison operations - return boolean
            Equal | NotEqual => Ok(Type::Primitive(PrimitiveType::Boolean)),

            Less | LessEqual | Greater | GreaterEqual => {
                if left_resolved.supports_comparison() && right_resolved.supports_comparison() {
                    Ok(Type::Primitive(PrimitiveType::Boolean))
                } else {
                    Err(TypeError::UnsupportedOperation {
                        type_name: format!(
                            "{} {} {}",
                            left_resolved.to_string(),
                            format!("{:?}", op),
                            right_resolved.to_string()
                        ),
                        operation: format!("{:?}", op),
                    })
                }
            }

            // Logical operations - require boolean types
            BoolAnd | BoolOr => match (&left_resolved, &right_resolved) {
                (
                    Type::Primitive(PrimitiveType::Boolean),
                    Type::Primitive(PrimitiveType::Boolean),
                ) => Ok(Type::Primitive(PrimitiveType::Boolean)),
                _ => Err(TypeError::UnsupportedOperation {
                    type_name: format!(
                        "{} {} {}",
                        left_resolved.to_string(),
                        format!("{:?}", op),
                        right_resolved.to_string()
                    ),
                    operation: format!("{:?}", op),
                }),
            },

            // Handle alternative names
            Subtract => self.infer_binary_result_type(left, right, Sub),
            Multiply => self.infer_binary_result_type(left, right, Mul),
            Divide => self.infer_binary_result_type(left, right, Div),
        }
    }

    /// Infer result type of unary operation
    fn infer_unary_result_type(
        &self,
        operand: &Type,
        op: UnaryOperator,
    ) -> Result<Type, TypeError> {
        use UnaryOperator::*;

        let operand_resolved = self.resolve_type(operand);

        match op {
            // Arithmetic unary operations
            Negate | Abs | Sign => match operand_resolved {
                Type::Primitive(PrimitiveType::Integer) => {
                    Ok(Type::Primitive(PrimitiveType::Integer))
                }
                _ => Err(TypeError::UnsupportedOperation {
                    type_name: operand_resolved.to_string(),
                    operation: format!("{:?}", op),
                }),
            },

            // Bitwise NOT
            Not | BitwiseNot => match operand_resolved {
                Type::Primitive(PrimitiveType::Integer) => {
                    Ok(Type::Primitive(PrimitiveType::Integer))
                }
                _ => Err(TypeError::UnsupportedOperation {
                    type_name: operand_resolved.to_string(),
                    operation: format!("{:?}", op),
                }),
            },

            // Logical NOT
            BoolNot => match operand_resolved {
                Type::Primitive(PrimitiveType::Boolean) => {
                    Ok(Type::Primitive(PrimitiveType::Boolean))
                }
                _ => Err(TypeError::UnsupportedOperation {
                    type_name: operand_resolved.to_string(),
                    operation: format!("{:?}", op),
                }),
            },

            // Mathematical functions
            Sqrt => match operand_resolved {
                Type::Primitive(PrimitiveType::Integer) => {
                    Ok(Type::Primitive(PrimitiveType::Integer))
                }
                _ => Err(TypeError::UnsupportedOperation {
                    type_name: operand_resolved.to_string(),
                    operation: format!("{:?}", op),
                }),
            },
        }
    }

    /// Infer result type of function call
    fn infer_call_result_type(
        &mut self,
        function: &str,
        arguments: &[Expression],
    ) -> Result<Type, TypeError> {
        // Check if it's a known function
        if let Some(func_type) = self.context.function_types.get(function) {
            if let Type::Function { return_type, .. } = func_type {
                return Ok((**return_type).clone());
            }
        }

        // Check if it's a syscall based on name pattern
        if function.starts_with("System.") {
            // Try to find syscall by name
            for syscall in self.context.syscall_types.values() {
                if syscall.name == function {
                    return Ok(syscall.return_type.clone());
                }
            }
        }

        // Try to parse as syscall hash
        if let Ok(hash) = u32::from_str_radix(function.trim_start_matches("0x"), 16) {
            if let Some(syscall) = self.context.syscall_types.get(&hash) {
                return Ok(syscall.return_type.clone());
            }
        }

        // Default to Any for unknown functions
        Ok(Type::Any)
    }

    /// Infer type of field access
    fn infer_field_access_type(
        &self,
        object_type: &Type,
        field_name: &str,
    ) -> Result<Type, TypeError> {
        let resolved_type = self.resolve_type(object_type);

        match resolved_type {
            Type::Struct(ref struct_type) => {
                for field in &struct_type.fields {
                    if field.name == field_name {
                        return Ok(field.field_type.clone());
                    }
                }
                Err(TypeError::FieldNotFound {
                    type_name: struct_type.name.clone().unwrap_or("Anonymous".to_string()),
                    field_name: field_name.to_string(),
                })
            }
            _ => Err(TypeError::FieldNotFound {
                type_name: resolved_type.to_string(),
                field_name: field_name.to_string(),
            }),
        }
    }

    /// Infer result type of array/map indexing
    fn infer_index_result_type(
        &self,
        container_type: &Type,
        index_type: &Type,
    ) -> Result<Type, TypeError> {
        let resolved_container = self.resolve_type(container_type);
        let resolved_index = self.resolve_type(index_type);

        match resolved_container {
            Type::Array(ref element_type) => {
                // Index should be integer
                if resolved_index.is_compatible_with(&Type::Primitive(PrimitiveType::Integer)) {
                    Ok((**element_type).clone())
                } else {
                    Err(TypeError::UnsupportedOperation {
                        type_name: resolved_index.to_string(),
                        operation: "array indexing".to_string(),
                    })
                }
            }
            Type::Map { ref value, .. } => {
                // Index type should be compatible with key type
                Ok((**value).clone())
            }
            Type::Buffer => {
                // Buffer indexing returns integer (byte value)
                if resolved_index.is_compatible_with(&Type::Primitive(PrimitiveType::Integer)) {
                    Ok(Type::Primitive(PrimitiveType::Integer))
                } else {
                    Err(TypeError::UnsupportedOperation {
                        type_name: resolved_index.to_string(),
                        operation: "buffer indexing".to_string(),
                    })
                }
            }
            Type::Primitive(PrimitiveType::ByteString) => {
                // ByteString indexing returns integer (byte value)
                if resolved_index.is_compatible_with(&Type::Primitive(PrimitiveType::Integer)) {
                    Ok(Type::Primitive(PrimitiveType::Integer))
                } else {
                    Err(TypeError::UnsupportedOperation {
                        type_name: resolved_index.to_string(),
                        operation: "string indexing".to_string(),
                    })
                }
            }
            _ => Err(TypeError::UnsupportedOperation {
                type_name: resolved_container.to_string(),
                operation: "indexing".to_string(),
            }),
        }
    }

    /// Check if a type supports a specific operation
    fn type_supports_operation(&self, ty: &Type, op: BinaryOperator) -> bool {
        use BinaryOperator::*;
        let resolved = self.resolve_type(ty);

        match op {
            // Arithmetic operations
            Add | Sub | Mul | Div | Mod | Pow | Subtract | Multiply | Divide => {
                match resolved {
                    Type::Primitive(PrimitiveType::Integer) => true,
                    // String concatenation for ADD
                    Type::Primitive(PrimitiveType::ByteString) if op == Add => true,
                    _ => false,
                }
            }

            // Bitwise operations
            And | Or | Xor | LeftShift | RightShift => {
                matches!(resolved, Type::Primitive(PrimitiveType::Integer))
            }

            // Comparison operations
            Equal | NotEqual => true, // All types can be compared for equality

            Less | LessEqual | Greater | GreaterEqual => resolved.supports_comparison(),

            // Logical operations
            BoolAnd | BoolOr => {
                matches!(resolved, Type::Primitive(PrimitiveType::Boolean))
            }
        }
    }

    /// Find a type compatible with the given operation
    fn find_operation_compatible_type(
        &self,
        ty: &Type,
        op: BinaryOperator,
    ) -> Result<Type, TypeError> {
        use BinaryOperator::*;

        match op {
            Add | Sub | Mul | Div | Mod | Pow | Subtract | Multiply | Divide => {
                Ok(Type::Primitive(PrimitiveType::Integer))
            }
            And | Or | Xor | LeftShift | RightShift => Ok(Type::Primitive(PrimitiveType::Integer)),
            Equal | NotEqual => Ok(ty.clone()), // Any type works
            Less | LessEqual | Greater | GreaterEqual => {
                Ok(Type::Primitive(PrimitiveType::Integer)) // Most common comparable type
            }
            BoolAnd | BoolOr => Ok(Type::Primitive(PrimitiveType::Boolean)),
        }
    }

    /// Check if one type is convertible to another (Neo N3 conversion rules)
    fn is_convertible(&self, from: &Type, to: &Type) -> bool {
        let from_resolved = self.resolve_type(from);
        let to_resolved = self.resolve_type(to);

        // Same type is always convertible
        if from_resolved == to_resolved {
            return true;
        }

        match (&from_resolved, &to_resolved) {
            // Any/Unknown types
            (Type::Unknown, _) | (_, Type::Unknown) => true,
            (Type::Any, _) | (_, Type::Any) => true,

            // Neo N3 specific conversions (based on CONVERT opcode semantics)

            // Integer conversions
            (Type::Primitive(PrimitiveType::Integer), Type::Primitive(PrimitiveType::Boolean)) => {
                true
            }
            (
                Type::Primitive(PrimitiveType::Integer),
                Type::Primitive(PrimitiveType::ByteString),
            ) => true,

            // Boolean conversions
            (Type::Primitive(PrimitiveType::Boolean), Type::Primitive(PrimitiveType::Integer)) => {
                true
            }
            (
                Type::Primitive(PrimitiveType::Boolean),
                Type::Primitive(PrimitiveType::ByteString),
            ) => true,

            // ByteString conversions
            (
                Type::Primitive(PrimitiveType::ByteString),
                Type::Primitive(PrimitiveType::Integer),
            ) => true,
            (
                Type::Primitive(PrimitiveType::ByteString),
                Type::Primitive(PrimitiveType::Boolean),
            ) => true,
            (Type::Primitive(PrimitiveType::ByteString), Type::Buffer) => true,

            // Buffer conversions
            (Type::Buffer, Type::Primitive(PrimitiveType::ByteString)) => true,

            // Array to ByteString (serialization)
            (Type::Array(_), Type::Primitive(PrimitiveType::ByteString)) => true,
            (Type::Primitive(PrimitiveType::ByteString), Type::Array(_)) => true,

            // Struct to ByteString (serialization)
            (Type::Struct(_), Type::Primitive(PrimitiveType::ByteString)) => true,
            (Type::Primitive(PrimitiveType::ByteString), Type::Struct(_)) => true,

            // Map to ByteString (serialization)
            (Type::Map { .. }, Type::Primitive(PrimitiveType::ByteString)) => true,
            (Type::Primitive(PrimitiveType::ByteString), Type::Map { .. }) => true,

            // Nullable unwrapping
            (Type::Nullable(inner), other) => self.is_convertible(inner, &other),
            (other, Type::Nullable(inner)) => self.is_convertible(&other, inner),

            // ECPoint to ByteString
            (
                Type::Primitive(PrimitiveType::ECPoint),
                Type::Primitive(PrimitiveType::ByteString),
            ) => true,
            (
                Type::Primitive(PrimitiveType::ByteString),
                Type::Primitive(PrimitiveType::ECPoint),
            ) => true,

            // Hash types to ByteString
            (
                Type::Primitive(PrimitiveType::Hash160),
                Type::Primitive(PrimitiveType::ByteString),
            ) => true,
            (
                Type::Primitive(PrimitiveType::Hash256),
                Type::Primitive(PrimitiveType::ByteString),
            ) => true,
            (
                Type::Primitive(PrimitiveType::ByteString),
                Type::Primitive(PrimitiveType::Hash160),
            ) => true,
            (
                Type::Primitive(PrimitiveType::ByteString),
                Type::Primitive(PrimitiveType::Hash256),
            ) => true,

            // PublicKey and Signature to ByteString
            (
                Type::Primitive(PrimitiveType::PublicKey),
                Type::Primitive(PrimitiveType::ByteString),
            ) => true,
            (
                Type::Primitive(PrimitiveType::Signature),
                Type::Primitive(PrimitiveType::ByteString),
            ) => true,
            (
                Type::Primitive(PrimitiveType::ByteString),
                Type::Primitive(PrimitiveType::PublicKey),
            ) => true,
            (
                Type::Primitive(PrimitiveType::ByteString),
                Type::Primitive(PrimitiveType::Signature),
            ) => true,

            _ => false,
        }
    }

    /// Apply inferred types back to the function
    pub fn apply_inferred_types(&mut self, function: &mut IRFunction) -> Result<(), TypeError> {
        // Apply types to local variables
        for local in &mut function.locals {
            if let Some(inferred_type) = self.context.get_variable_type(&local.name) {
                let resolved_type = self.resolve_type(inferred_type);
                if resolved_type != Type::Unknown && resolved_type != Type::Variable(0) {
                    local.local_type = resolved_type.clone();
                    // Update var_type if it was unknown
                    if local.var_type == Type::Unknown {
                        local.var_type = resolved_type;
                    }
                }
            }
        }

        // Apply types to parameters
        for param in &mut function.parameters {
            if let Some(inferred_type) = self.context.get_variable_type(&param.name) {
                let resolved_type = self.resolve_type(inferred_type);
                if resolved_type != Type::Unknown && resolved_type != Type::Variable(0) {
                    // Update param_type if it was unknown
                    if param.param_type == Type::Unknown {
                        param.param_type = resolved_type;
                    }
                }
            }
        }

        Ok(())
    }

    /// Get inference statistics
    pub fn get_stats(&self) -> &InferenceStats {
        &self.context.stats
    }

    /// Reset the inference context for reuse
    pub fn reset(&mut self) {
        self.context = TypeInferenceContext::new();
        Self::init_syscall_database(&mut self.context);
    }

    /// Add custom function type signature
    pub fn add_function_signature(&mut self, name: String, signature: Type) {
        self.context.function_types.insert(name, signature);
    }

    /// Add storage key type pattern
    pub fn add_storage_type(&mut self, key_pattern: String, value_type: Type) {
        self.context.storage_types.insert(key_pattern, value_type);
    }

    /// Export type information for external use
    pub fn export_type_info(&self) -> HashMap<String, Type> {
        let mut exported = HashMap::new();

        // Export resolved variable types
        for (name, ty) in &self.context.variable_types {
            let resolved = self.resolve_type(ty);
            if resolved != Type::Unknown {
                exported.insert(name.clone(), resolved);
            }
        }

        exported
    }

    /// Create type annotation for given type
    pub fn create_type_annotation(&self, ty: &Type) -> String {
        let resolved = self.resolve_type(ty);
        self.format_type_annotation(&resolved)
    }

    /// Format type annotation string
    fn format_type_annotation(&self, ty: &Type) -> String {
        match ty {
            Type::Primitive(prim) => match prim {
                PrimitiveType::Boolean => "bool".to_string(),
                PrimitiveType::Integer => "int".to_string(),
                PrimitiveType::ByteString => "bytes".to_string(),
                PrimitiveType::Hash160 => "Hash160".to_string(),
                PrimitiveType::Hash256 => "Hash256".to_string(),
                PrimitiveType::ECPoint => "ECPoint".to_string(),
                PrimitiveType::PublicKey => "PublicKey".to_string(),
                PrimitiveType::Signature => "Signature".to_string(),
                PrimitiveType::Null => "null".to_string(),
                PrimitiveType::String => "string".to_string(),
                PrimitiveType::ByteArray => "ByteArray".to_string(),
            },
            Type::Array(elem) => format!("{}[]", self.format_type_annotation(elem)),
            Type::Map { key, value } => format!(
                "Map<{}, {}>",
                self.format_type_annotation(key),
                self.format_type_annotation(value)
            ),
            Type::Buffer => "Buffer".to_string(),
            Type::Struct(s) => s.name.clone().unwrap_or("struct".to_string()),
            Type::Nullable(inner) => format!("{}?", self.format_type_annotation(inner)),
            Type::Union(types) => {
                let formatted: Vec<String> = types
                    .iter()
                    .map(|t| self.format_type_annotation(t))
                    .collect();
                format!("({})", formatted.join(" | "))
            }
            Type::Contract(c) => c.name.clone(),
            Type::InteropInterface(name) => format!("InteropInterface<{}>", name),
            Type::Any => "any".to_string(),
            Type::Unknown => "unknown".to_string(),
            _ => ty.to_string(),
        }
    }

    /// Extract type metadata from inferred types
    pub fn extract_type_metadata(&self) -> TypeMetadata {
        let mut metadata = TypeMetadata::new();

        // Extract variable type information
        for (name, ty) in &self.context.variable_types {
            let resolved = self.resolve_type(ty);
            if resolved != Type::Unknown {
                metadata
                    .variable_types
                    .insert(name.clone(), resolved.clone());
                metadata
                    .type_annotations
                    .insert(name.clone(), self.format_type_annotation(&resolved));
            }
        }

        // Extract function signatures
        for (name, ty) in &self.context.function_types {
            metadata
                .function_signatures
                .insert(name.clone(), ty.clone());
        }

        // Extract storage patterns
        for (pattern, ty) in &self.context.storage_types {
            metadata
                .storage_patterns
                .insert(pattern.clone(), ty.clone());
        }

        metadata.inference_stats = self.context.stats.clone();

        metadata
    }

    /// Helper method to integrate with LocalVariable updates
    pub fn update_local_variable_type(&mut self, variable_name: &str, inferred_type: Type) {
        self.context
            .set_variable_type(variable_name.to_string(), inferred_type);
    }

    /// Bulk update variable types from external source
    pub fn bulk_update_variable_types(&mut self, types: HashMap<String, Type>) {
        for (name, ty) in types {
            self.context.set_variable_type(name, ty);
        }
    }

    /// Check if type inference is complete (no unresolved variables)
    pub fn is_inference_complete(&self) -> bool {
        for ty in self.context.variable_types.values() {
            let resolved = self.resolve_type(ty);
            match resolved {
                Type::Variable(_) | Type::Unknown => return false,
                _ => {}
            }
        }
        true
    }

    /// Get unresolved type variables
    pub fn get_unresolved_variables(&self) -> Vec<String> {
        let mut unresolved = Vec::new();

        for (name, ty) in &self.context.variable_types {
            let resolved = self.resolve_type(ty);
            match resolved {
                Type::Variable(_) | Type::Unknown => {
                    unresolved.push(name.clone());
                }
                _ => {}
            }
        }

        unresolved
    }

    /// Create struct type from field information
    pub fn create_struct_type(
        &mut self,
        name: Option<String>,
        fields: Vec<(String, Type)>,
    ) -> Type {
        let field_types = fields
            .into_iter()
            .enumerate()
            .map(|(i, (field_name, field_type))| FieldType {
                name: field_name,
                field_type,
                index: i,
                optional: false,
            })
            .collect();

        Type::Struct(StructType {
            name,
            fields: field_types,
            is_packed: false,
        })
    }

    /// Analyze generic type instantiation
    pub fn instantiate_generic_type(&mut self, base_type: &str, type_params: Vec<Type>) -> Type {
        Type::Generic {
            base: base_type.to_string(),
            parameters: type_params,
        }
    }
}

/// Type metadata extracted from inference
#[derive(Debug, Clone, Default)]
pub struct TypeMetadata {
    /// Variable name to type mapping
    pub variable_types: HashMap<String, Type>,
    /// Variable name to type annotation string
    pub type_annotations: HashMap<String, String>,
    /// Function signatures
    pub function_signatures: HashMap<String, Type>,
    /// Storage key patterns
    pub storage_patterns: HashMap<String, Type>,
    /// Inference statistics
    pub inference_stats: InferenceStats,
}

impl TypeMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get type annotation for variable
    pub fn get_annotation(&self, variable_name: &str) -> Option<&String> {
        self.type_annotations.get(variable_name)
    }

    /// Check if variable has known type
    pub fn has_type_info(&self, variable_name: &str) -> bool {
        self.variable_types.contains_key(variable_name)
    }
}

impl Default for TypeInferenceContext {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for TypeInferenceEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Display implementation for Type
impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ir::{IRBlock, IRFunction, LocalVariable, Parameter};

    #[test]
    fn test_type_compatibility() {
        let int_type = Type::Primitive(PrimitiveType::Integer);
        let bool_type = Type::Primitive(PrimitiveType::Boolean);
        let unknown_type = Type::Unknown;

        assert!(int_type.is_compatible_with(&int_type));
        assert!(!int_type.is_compatible_with(&bool_type));
        assert!(int_type.is_compatible_with(&unknown_type));
        assert!(unknown_type.is_compatible_with(&bool_type));
    }

    #[test]
    fn test_type_properties() {
        let int_type = Type::Primitive(PrimitiveType::Integer);
        let bool_type = Type::Primitive(PrimitiveType::Boolean);
        let string_type = Type::Primitive(PrimitiveType::ByteString);
        let array_type = Type::Array(Box::new(int_type.clone()));
        let nullable_type = Type::Nullable(Box::new(int_type.clone()));

        assert!(int_type.is_numeric());
        assert!(!int_type.is_boolean());
        assert!(int_type.supports_arithmetic());
        assert!(int_type.supports_comparison());

        assert!(bool_type.is_boolean());
        assert!(!bool_type.is_numeric());
        assert!(!bool_type.supports_arithmetic());

        assert!(string_type.is_string());
        assert!(string_type.supports_comparison());

        assert!(array_type.is_container());
        assert!(array_type.supports_indexing());

        assert!(nullable_type.is_nullable());
    }

    #[test]
    fn test_type_inference_context() {
        let mut ctx = TypeInferenceContext::new();

        let var1 = ctx.fresh_type_var();
        let var2 = ctx.fresh_type_var();

        assert_ne!(var1, var2);
        assert_eq!(ctx.constraints.len(), 0);

        ctx.add_constraint(TypeConstraint::Equal(var1.clone(), var2.clone()));
        assert_eq!(ctx.constraints.len(), 1);

        // Test variable type management
        ctx.set_variable_type("x".to_string(), Type::Primitive(PrimitiveType::Integer));
        assert_eq!(
            ctx.get_variable_type("x"),
            Some(&Type::Primitive(PrimitiveType::Integer))
        );
    }

    #[test]
    fn test_type_subtyping() {
        let int_type = Type::Primitive(PrimitiveType::Integer);
        let any_type = Type::Any;
        let never_type = Type::Never;
        let nullable_int = Type::Nullable(Box::new(int_type.clone()));
        let null_type = Type::Primitive(PrimitiveType::Null);

        // Any is supertype of everything
        assert!(int_type.is_subtype_of(&any_type));

        // Never is subtype of everything
        assert!(never_type.is_subtype_of(&int_type));

        // Null is subtype of nullable types
        assert!(null_type.is_subtype_of(&nullable_int));

        // Exact matches
        assert!(int_type.is_subtype_of(&int_type));
    }

    #[test]
    fn test_type_utility_functions() {
        // Test array type creation
        let array_type = Type::array(Type::Primitive(PrimitiveType::Integer));
        match array_type {
            Type::Array(elem_type) => {
                assert_eq!(*elem_type, Type::Primitive(PrimitiveType::Integer));
            }
            _ => assert!(false, "Expected array type"),
        }

        // Test map type creation
        let map_type = Type::map(
            Type::Primitive(PrimitiveType::ByteString),
            Type::Primitive(PrimitiveType::Integer),
        );
        match map_type {
            Type::Map { key, value } => {
                assert_eq!(*key, Type::Primitive(PrimitiveType::ByteString));
                assert_eq!(*value, Type::Primitive(PrimitiveType::Integer));
            }
            _ => assert!(false, "Expected map type"),
        }

        // Test nullable type creation
        let nullable_type = Type::nullable(Type::Primitive(PrimitiveType::Integer));
        match nullable_type {
            Type::Nullable(inner) => {
                assert_eq!(*inner, Type::Primitive(PrimitiveType::Integer));
            }
            _ => assert!(false, "Expected nullable type"),
        }
    }

    #[test]
    fn test_syscall_type_database() {
        let engine = TypeInferenceEngine::new();

        // Test syscall signature lookup
        let storage_get = engine.context.get_syscall_type(0x0c40166b);
        assert!(storage_get.is_some());

        let sig = storage_get.unwrap();
        assert_eq!(sig.name, "System.Storage.Get");
        assert_eq!(sig.parameters.len(), 1);
        assert_eq!(
            sig.parameters[0],
            Type::Primitive(PrimitiveType::ByteString)
        );

        match &sig.return_type {
            Type::Nullable(inner) => {
                assert_eq!(**inner, Type::Primitive(PrimitiveType::ByteString));
            }
            _ => assert!(false, "Expected nullable ByteString return type"),
        }
    }

    #[test]
    fn test_error_handling() {
        let mut ctx = TypeInferenceContext::new();

        // Test error addition
        let error = TypeError::UndefinedVariable {
            name: "unknown_var".to_string(),
        };

        ctx.add_error(error.clone());
        assert!(ctx.has_errors());
        assert_eq!(ctx.get_errors().len(), 1);
        assert_eq!(ctx.get_errors()[0], error);
    }

    #[test]
    fn test_type_environment_scoping() {
        let mut ctx = TypeInferenceContext::new();

        // Set variable in global scope
        ctx.set_variable_type(
            "global_var".to_string(),
            Type::Primitive(PrimitiveType::Integer),
        );
        assert_eq!(
            ctx.get_variable_type("global_var"),
            Some(&Type::Primitive(PrimitiveType::Integer))
        );

        // Push new scope
        ctx.push_scope();
        ctx.set_variable_type(
            "local_var".to_string(),
            Type::Primitive(PrimitiveType::Boolean),
        );

        // Should find both variables
        assert_eq!(
            ctx.get_variable_type("global_var"),
            Some(&Type::Primitive(PrimitiveType::Integer))
        );
        assert_eq!(
            ctx.get_variable_type("local_var"),
            Some(&Type::Primitive(PrimitiveType::Boolean))
        );

        // Pop scope
        ctx.pop_scope();

        // Should still find global variable
        assert_eq!(
            ctx.get_variable_type("global_var"),
            Some(&Type::Primitive(PrimitiveType::Integer))
        );
        // But local variable should still be accessible via global storage
        assert_eq!(
            ctx.get_variable_type("local_var"),
            Some(&Type::Primitive(PrimitiveType::Boolean))
        );
    }
}
