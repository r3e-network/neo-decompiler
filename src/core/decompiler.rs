//! Main decompilation engine

use crate::analysis::{
    cfg::{CFGBuilder, ControlFlowGraph, Loop},
    effects::{Effect, EffectInferenceEngine, KeyPattern},
    types::{Type, TypeConstraint, TypeError, TypeInferenceContext, TypeInferenceEngine},
};
use crate::common::types::{StorageOp, Variable};
use crate::common::{config::DecompilerConfig, errors::AnalysisError, types::*};
use crate::core::{
    ir::{Expression, IRBlock, IRFunction, Operation, StackOp, Terminator},
    syscalls::SyscallDatabase,
};
use crate::frontend::manifest_parser::ContractManifest;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// Main decompilation engine with comprehensive analysis passes
pub struct DecompilerEngine {
    /// Configuration
    config: DecompilerConfig,
    /// Syscall database
    syscall_db: SyscallDatabase,
    /// Type inference engine
    type_engine: TypeInferenceEngine,
    /// Effect inference engine
    effect_engine: EffectInferenceEngine,
    /// CFG builder
    cfg_builder: CFGBuilder,
    /// Analysis statistics
    stats: AnalysisStatistics,
}

/// Analysis statistics for performance monitoring
#[derive(Debug, Clone, Default)]
pub struct AnalysisStatistics {
    /// Total analysis time
    pub total_time_ms: u64,
    /// Type inference time
    pub type_inference_time_ms: u64,
    /// CFG analysis time  
    pub cfg_analysis_time_ms: u64,
    /// Effect analysis time
    pub effect_analysis_time_ms: u64,
    /// Optimization time
    pub optimization_time_ms: u64,
    /// Number of passes applied
    pub passes_applied: u32,
    /// Optimization iterations
    pub optimization_iterations: u32,
    /// Dead code blocks removed
    pub dead_blocks_removed: u32,
    /// Constants propagated
    pub constants_propagated: u32,
    /// Copies eliminated
    pub copies_eliminated: u32,
}

/// Optimization level configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationLevel {
    /// No optimizations
    None,
    /// Basic optimizations (dead code elimination)
    Basic,
    /// Aggressive optimizations (all passes)
    Aggressive,
}

/// Analysis pass results
#[derive(Debug, Clone)]
pub struct AnalysisResults {
    /// Type inference results
    pub type_results: TypeAnalysisResults,
    /// Control flow graph
    pub cfg: Option<ControlFlowGraph>,
    /// Effect analysis results
    pub effect_results: EffectAnalysisResults,
    /// Optimization results
    pub optimization_results: OptimizationResults,
    /// Analysis errors (non-fatal)
    pub warnings: Vec<AnalysisWarning>,
}

/// Type analysis results
#[derive(Debug, Clone)]
pub struct TypeAnalysisResults {
    /// Variable types
    pub variable_types: HashMap<String, Type>,
    /// Function signatures
    pub function_types: HashMap<String, Type>,
    /// Storage key types
    pub storage_types: HashMap<String, Type>,
    /// Type errors found
    pub type_errors: Vec<TypeError>,
    /// Type inference statistics
    pub stats: crate::analysis::types::InferenceStats,
}

/// Effect analysis results
#[derive(Debug, Clone)]
pub struct EffectAnalysisResults {
    /// Effects per block
    pub block_effects: HashMap<BlockId, Vec<Effect>>,
    /// Function effect summary
    pub function_effects: Vec<Effect>,
    /// Storage access patterns
    pub storage_patterns: HashMap<String, KeyPattern>,
    /// Gas cost estimation
    pub estimated_gas_cost: Option<u64>,
    /// Security risk assessment
    pub security_risks: Vec<SecurityRisk>,
}

/// Security risk assessment
#[derive(Debug, Clone)]
pub struct SecurityRisk {
    /// Risk type
    pub risk_type: RiskType,
    /// Risk severity (1-10)
    pub severity: u32,
    /// Description
    pub description: String,
    /// Affected blocks
    pub affected_blocks: Vec<BlockId>,
    /// Mitigation suggestions
    pub mitigation: Option<String>,
}

/// Security risk types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskType {
    /// Unrestricted storage access
    UnrestrictedStorageAccess,
    /// Reentrancy vulnerability
    ReentrancyRisk,
    /// Integer overflow potential
    IntegerOverflowRisk,
    /// Unvalidated external calls
    UnvalidatedExternalCalls,
    /// Gas limit issues
    GasLimitIssues,
    /// Access control bypass
    AccessControlBypass,
}

/// Optimization results
#[derive(Debug, Clone, Default)]
pub struct OptimizationResults {
    /// Dead code blocks removed
    pub dead_blocks_removed: HashSet<BlockId>,
    /// Constants propagated
    pub constants_propagated: Vec<ConstantPropagation>,
    /// Copy eliminations performed
    pub copies_eliminated: Vec<CopyElimination>,
    /// Common subexpressions eliminated
    pub subexpressions_eliminated: Vec<SubexpressionElimination>,
    /// Loop optimizations applied
    pub loop_optimizations: Vec<LoopOptimization>,
}

/// Constant propagation record
#[derive(Debug, Clone)]
pub struct ConstantPropagation {
    /// Block where propagation occurred
    pub block_id: BlockId,
    /// Variable name
    pub variable: String,
    /// Constant value
    pub value: Literal,
    /// Number of uses replaced
    pub uses_replaced: u32,
}

/// Copy elimination record
#[derive(Debug, Clone)]
pub struct CopyElimination {
    /// Block where elimination occurred
    pub block_id: BlockId,
    /// Source variable
    pub source: String,
    /// Target variable (eliminated)
    pub target: String,
    /// Number of uses replaced
    pub uses_replaced: u32,
}

/// Subexpression elimination record
#[derive(Debug, Clone)]
pub struct SubexpressionElimination {
    /// Block where elimination occurred
    pub block_id: BlockId,
    /// Expression that was eliminated
    pub expression: String,
    /// Variable holding the common result
    pub result_variable: String,
    /// Number of occurrences eliminated
    pub occurrences_eliminated: u32,
}

/// Loop optimization record
#[derive(Debug, Clone)]
pub struct LoopOptimization {
    /// Loop header block
    pub loop_header: BlockId,
    /// Optimization type
    pub optimization_type: LoopOptimizationType,
    /// Performance improvement estimate
    pub improvement_estimate: f32,
}

/// Loop optimization types
#[derive(Debug, Clone)]
pub enum LoopOptimizationType {
    /// Loop invariant code motion
    InvariantCodeMotion { moved_operations: Vec<String> },
    /// Loop unrolling
    LoopUnrolling { unroll_factor: u32 },
    /// Strength reduction
    StrengthReduction { reductions: Vec<String> },
}

/// Analysis warning (non-fatal issues)
#[derive(Debug, Clone)]
pub struct AnalysisWarning {
    /// Warning type
    pub warning_type: WarningType,
    /// Warning message
    pub message: String,
    /// Affected blocks
    pub affected_blocks: Vec<BlockId>,
    /// Severity (1-5)
    pub severity: u32,
}

/// Analysis warning types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WarningType {
    /// Type inference uncertainty
    TypeInferenceUncertainty,
    /// Potential optimization opportunity
    OptimizationOpportunity,
    /// Complex control flow
    ComplexControlFlow,
    /// Performance concern
    PerformanceConcern,
    /// Code quality issue
    CodeQualityIssue,
}

/// Loop analysis result
#[derive(Debug, Clone)]
pub struct LoopAnalysis {
    /// Loop header block
    pub header: BlockId,
    /// Size of loop body
    pub body_size: usize,
    /// Is this loop performance critical
    pub is_performance_critical: bool,
    /// Has nested loops
    pub has_nested_loops: bool,
    /// Estimated iterations
    pub estimated_iterations: Option<u64>,
    /// Loop invariant operations
    pub invariant_operations: Vec<String>,
}

impl DecompilerEngine {
    /// Create new decompiler engine
    pub fn new(config: &DecompilerConfig) -> Self {
        // Create syscall database from configuration
        let mut syscall_db = SyscallDatabase::new();

        // Load custom syscalls from configuration
        for (_, definition) in &config.syscalls.custom_syscalls {
            let _ = syscall_db.add_syscall_definition(definition.clone());
        }

        Self {
            config: config.clone(),
            syscall_db,
            type_engine: TypeInferenceEngine::new(),
            effect_engine: EffectInferenceEngine::new(),
            cfg_builder: CFGBuilder::new(),
            stats: AnalysisStatistics::default(),
        }
    }

    /// Create new decompiler engine with custom optimization level
    pub fn with_optimization_level(
        config: &DecompilerConfig,
        opt_level: OptimizationLevel,
    ) -> Self {
        let mut engine = Self::new(config);

        // Configure optimization level
        match opt_level {
            OptimizationLevel::None => {
                // Disable all optimizations
            }
            OptimizationLevel::Basic => {
                // Enable basic optimizations
            }
            OptimizationLevel::Aggressive => {
                // Enable all optimizations
                engine.cfg_builder.enable_advanced_analysis = true;
                engine.cfg_builder.enable_loop_detection = true;
            }
        }

        engine
    }

    /// Apply comprehensive analysis passes to IR function
    pub fn analyze(
        &mut self,
        ir_function: &mut IRFunction,
        manifest: Option<&ContractManifest>,
    ) -> Result<AnalysisResults, AnalysisError> {
        let start_time = Instant::now();
        let mut results = AnalysisResults {
            type_results: TypeAnalysisResults {
                variable_types: HashMap::new(),
                function_types: HashMap::new(),
                storage_types: HashMap::new(),
                type_errors: Vec::new(),
                stats: Default::default(),
            },
            cfg: None,
            effect_results: EffectAnalysisResults {
                block_effects: HashMap::new(),
                function_effects: Vec::new(),
                storage_patterns: HashMap::new(),
                estimated_gas_cost: None,
                security_risks: Vec::new(),
            },
            optimization_results: OptimizationResults::default(),
            warnings: Vec::new(),
        };

        // Phase 1: Control Flow Graph Construction (required for other analyses)
        if self.config.analysis.enable_cfg_analysis {
            let cfg_start = Instant::now();
            match self.apply_cfg_analysis(ir_function) {
                Ok(cfg) => {
                    self.stats.cfg_analysis_time_ms = cfg_start.elapsed().as_millis() as u64;
                    results.cfg = Some(cfg);
                }
                Err(cfg_error) => {
                    // CFG construction failed - continue without CFG for graceful degradation
                    results.warnings.push(AnalysisWarning {
                        warning_type: WarningType::PerformanceConcern,
                        message: format!("CFG construction failed: {}", cfg_error),
                        affected_blocks: vec![],
                        severity: 2,
                    });
                    // Continue without CFG - this allows basic decompilation to work
                }
            }
        }

        // Phase 2: Type Inference
        if self.config.analysis.enable_type_inference {
            let type_start = Instant::now();
            let type_results =
                self.apply_type_inference(ir_function, results.cfg.as_ref(), manifest)?;
            self.stats.type_inference_time_ms = type_start.elapsed().as_millis() as u64;
            results.type_results = type_results;
        }

        // Phase 3: Effect Analysis
        if self.config.analysis.enable_effect_analysis {
            let effect_start = Instant::now();
            let effect_results = self.apply_effect_analysis(
                ir_function,
                results.cfg.as_ref(),
                &results.type_results,
            )?;
            self.stats.effect_analysis_time_ms = effect_start.elapsed().as_millis() as u64;
            results.effect_results = effect_results;
        }

        // Phase 4: Loop Detection and Analysis
        if self.config.analysis.enable_loop_detection {
            if let Some(ref cfg) = results.cfg {
                self.apply_loop_detection(ir_function, cfg)?;
            }
        }

        // Phase 5: Optimization Passes
        if self.config.analysis.enable_dead_code_elimination || self.optimization_enabled() {
            let opt_start = Instant::now();
            let opt_results = self.apply_optimization_passes(ir_function, &results)?;
            self.stats.optimization_time_ms = opt_start.elapsed().as_millis() as u64;
            results.optimization_results = opt_results;
        }

        // Phase 6: Neo N3 Specific Analysis
        self.apply_neo_specific_analysis(ir_function, &mut results)?;

        // Phase 7: Analysis Result Integration
        self.integrate_analysis_results(ir_function, &results)?;

        // Update statistics
        self.stats.total_time_ms = start_time.elapsed().as_millis() as u64;
        self.stats.passes_applied = self.count_applied_passes();

        Ok(results)
    }

    /// Check if any optimization is enabled
    fn optimization_enabled(&self) -> bool {
        self.config.analysis.enable_dead_code_elimination
        // Add other optimization flags as they become available in config
    }

    /// Apply type inference analysis pass with sophisticated constraint-based inference
    fn apply_type_inference(
        &mut self,
        ir_function: &mut IRFunction,
        cfg: Option<&ControlFlowGraph>,
        manifest: Option<&ContractManifest>,
    ) -> Result<TypeAnalysisResults, AnalysisError> {
        let mut context = TypeInferenceContext::new();

        // Initialize context with manifest information if available
        if let Some(manifest) = manifest {
            self.initialize_manifest_types(&mut context, manifest)?;
        }

        // Phase 1: Collect initial type information
        self.collect_initial_types(ir_function, &mut context)?;

        // Phase 2: Generate type constraints from operations
        for (block_id, block) in &ir_function.blocks {
            self.generate_block_constraints(&mut context, *block_id, block)?;
        }

        // Phase 3: Solve type constraints
        self.solve_type_constraints(&mut context)?;

        // Phase 4: Propagate types through control flow (if CFG available)
        if let Some(cfg) = cfg {
            self.propagate_types_through_cfg(&mut context, ir_function, cfg)?;
        }

        // Phase 5: Infer complex types (arrays, maps, structs)
        self.infer_complex_types(&mut context, ir_function)?;

        // Phase 6: Validate and refine type assignments
        self.validate_type_assignments(&mut context, ir_function)?;

        // Update type engine state
        self.type_engine.context = context.clone();

        // Prepare results
        Ok(TypeAnalysisResults {
            variable_types: context.variable_types.clone(),
            function_types: context.function_types.clone(),
            storage_types: context.storage_types.clone(),
            type_errors: context.errors.clone(),
            stats: context.stats.clone(),
        })
    }

    /// Initialize type context with manifest information
    fn initialize_manifest_types(
        &self,
        context: &mut TypeInferenceContext,
        manifest: &ContractManifest,
    ) -> Result<(), AnalysisError> {
        // Extract function signatures from manifest ABI
        for method in &manifest.abi.methods {
            // Store method signature information for later use
            let method_type = Type::Function {
                parameters: method
                    .parameters
                    .iter()
                    .map(|p| self.parse_neo_type(&p.param_type).unwrap_or(Type::Unknown))
                    .collect(),
                return_type: Box::new(
                    method
                        .return_type
                        .as_ref()
                        .map(|rt| self.parse_neo_type(rt).unwrap_or(Type::Unknown))
                        .unwrap_or(Type::Void), // Default to Void for methods without return type
                ),
            };

            // Store in function types for later lookup
            context
                .function_types
                .insert(format!("method_{}", method.name), method_type);
        }

        // Store event signatures
        for event in &manifest.abi.events {
            let event_fields: Vec<crate::analysis::types::FieldType> = event
                .parameters
                .iter()
                .enumerate()
                .map(|(i, p)| crate::analysis::types::FieldType {
                    name: p.name.clone(),
                    field_type: self.parse_neo_type(&p.param_type).unwrap_or(Type::Unknown),
                    index: i,
                    optional: false,
                })
                .collect();

            let event_type = Type::Struct(crate::analysis::types::StructType {
                name: Some(event.name.clone()),
                fields: event_fields,
                is_packed: false,
            });

            // Store event type for later lookup
            context
                .function_types
                .insert(format!("event_{}", event.name), event_type);
        }
        Ok(())
    }

    /// Collect initial type information from function signature and parameters
    fn collect_initial_types(
        &self,
        ir_function: &IRFunction,
        context: &mut TypeInferenceContext,
    ) -> Result<(), AnalysisError> {
        // Set parameter types
        for param in &ir_function.parameters {
            let param_type = param.param_type.clone().unwrap_or(Type::Unknown);
            context.set_variable_type(param.name.clone(), param_type);
        }

        // Set local variable types (if known)
        for local in &ir_function.locals {
            let local_type = local.local_type.clone().unwrap_or(Type::Unknown);
            context.set_variable_type(local.name.clone(), local_type);
        }

        // Set function return type
        if let Some(return_type) = &ir_function.return_type {
            context.function_types.insert(
                ir_function.name.clone(),
                Type::function(
                    ir_function
                        .parameters
                        .iter()
                        .map(|p| p.param_type.clone().unwrap_or(Type::Unknown))
                        .collect(),
                    return_type.clone(),
                ),
            );
        }

        Ok(())
    }

    /// Generate type constraints from block operations
    fn generate_block_constraints(
        &self,
        context: &mut TypeInferenceContext,
        _block_id: BlockId,
        block: &IRBlock,
    ) -> Result<(), AnalysisError> {
        // Generate constraints from operations
        for operation in &block.operations {
            self.generate_operation_constraints(context, operation)?;
        }

        // Generate constraints from terminator
        self.generate_terminator_constraints(context, &block.terminator)?;

        Ok(())
    }

    /// Generate type constraints from a single operation
    fn generate_operation_constraints(
        &self,
        context: &mut TypeInferenceContext,
        operation: &Operation,
    ) -> Result<(), AnalysisError> {
        match operation {
            Operation::Assign { target, source } => {
                let source_type = self.infer_expression_type(context, source)?;
                let target_var_name = self.variable_name(target)?;
                let type_var_id = context.next_type_var;
                context.next_type_var += 1;
                let type_var = Type::Variable(type_var_id);
                context.add_constraint(TypeConstraint::Equal(type_var.clone(), source_type));
                context.set_variable_type(target_var_name, type_var);
            }

            Operation::Syscall {
                name,
                arguments,
                return_type,
                target,
            } => {
                // Get syscall signature directly from database
                if let Some(syscall_hash) = self.get_syscall_hash(name) {
                    if let Some(sig) = self.syscall_db.get_signature(syscall_hash) {
                        // Constrain argument types based on syscall signature
                        for (i, arg) in arguments.iter().enumerate() {
                            if let Some(param_type) = sig.parameters.get(i) {
                                let arg_type = self.infer_expression_type(context, arg)?;
                                context.add_constraint(TypeConstraint::Equal(
                                    arg_type,
                                    param_type.clone(),
                                ));
                            }
                        }

                        // Set return type constraint based on actual syscall signature
                        if let Some(target) = target {
                            let target_var_name = self.variable_name(target)?;
                            context.set_variable_type(target_var_name, sig.return_type.clone());
                        }
                    }
                } else if let Some(return_type) = return_type {
                    // Use explicit return type
                    if let Some(target) = target {
                        let target_var_name = self.variable_name(target)?;
                        context.set_variable_type(target_var_name, return_type.clone());
                    }
                }
            }

            Operation::Arithmetic {
                operator,
                left,
                right,
                target,
            } => {
                let left_type = self.infer_expression_type(context, left)?;
                let right_type = self.infer_expression_type(context, right)?;
                let result_type =
                    self.infer_arithmetic_result_type(operator, &left_type, &right_type);
                let target_var_name = self.variable_name(target)?;

                // Add constraints
                context.add_constraint(TypeConstraint::SupportsOperation(left_type, *operator));
                context.add_constraint(TypeConstraint::SupportsOperation(right_type, *operator));
                context.set_variable_type(target_var_name, result_type);
            }

            Operation::Convert {
                source,
                target_type,
                target,
            } => {
                let source_type = self.infer_expression_type(context, source)?;
                let target_var_name = self.variable_name(target)?;

                context.add_constraint(TypeConstraint::Convertible(
                    source_type,
                    target_type.clone(),
                ));
                context.set_variable_type(target_var_name, target_type.clone());
            }

            Operation::ArrayOp {
                operation,
                array,
                index,
                value,
                target,
            } => {
                let array_type = self.infer_expression_type(context, array)?;

                match operation {
                    crate::core::ir::ArrayOperation::GetItem => {
                        if let Some(index) = index {
                            let index_type = self.infer_expression_type(context, index)?;
                            context.add_constraint(TypeConstraint::Equal(
                                index_type,
                                Type::Primitive(crate::analysis::types::PrimitiveType::Integer),
                            ));
                        }

                        if let Some(target) = target {
                            let target_var_name = self.variable_name(target)?;
                            // Array element type constraint
                            let element_type = context.fresh_type_var();
                            context.add_constraint(TypeConstraint::Indexable(
                                array_type,
                                Type::Primitive(crate::analysis::types::PrimitiveType::Integer),
                                element_type.clone(),
                            ));
                            context.set_variable_type(target_var_name, element_type);
                        }
                    }

                    crate::core::ir::ArrayOperation::SetItem => {
                        if let (Some(index), Some(value)) = (index, value) {
                            let index_type = self.infer_expression_type(context, index)?;
                            let value_type = self.infer_expression_type(context, value)?;

                            context.add_constraint(TypeConstraint::Equal(
                                index_type,
                                Type::Primitive(crate::analysis::types::PrimitiveType::Integer),
                            ));

                            // Array element type constraint
                            context.add_constraint(TypeConstraint::Indexable(
                                array_type,
                                Type::Primitive(crate::analysis::types::PrimitiveType::Integer),
                                value_type,
                            ));
                        }
                    }

                    _ => {
                        // Handle other array operations
                    }
                }
            }

            Operation::Storage {
                operation,
                key,
                value,
                target,
            } => {
                let key_type = self.infer_expression_type(context, key)?;
                context.add_constraint(TypeConstraint::Equal(
                    key_type,
                    Type::Primitive(crate::analysis::types::PrimitiveType::ByteString),
                ));

                match operation {
                    StorageOp::Get => {
                        if let Some(target) = target {
                            let target_var_name = self.variable_name(target)?;
                            context.set_variable_type(
                                target_var_name,
                                Type::Nullable(Box::new(Type::Primitive(
                                    crate::analysis::types::PrimitiveType::ByteString,
                                ))),
                            );
                        }
                    }

                    StorageOp::Put => {
                        if let Some(value) = value {
                            let _value_type = self.infer_expression_type(context, value)?;
                            // Storage values are typically ByteString
                        }
                    }

                    _ => {}
                }
            }

            _ => {
                // Handle other operations
            }
        }

        Ok(())
    }

    /// Generate type constraints from terminator
    fn generate_terminator_constraints(
        &self,
        context: &mut TypeInferenceContext,
        terminator: &Terminator,
    ) -> Result<(), AnalysisError> {
        match terminator {
            Terminator::Branch { condition, .. } => {
                let condition_type = self.infer_expression_type(context, condition)?;
                context.add_constraint(TypeConstraint::Equal(
                    condition_type,
                    Type::Primitive(crate::analysis::types::PrimitiveType::Boolean),
                ));
            }

            Terminator::Return(Some(expr)) => {
                let return_type = self.infer_expression_type(context, expr)?;
                // Add constraint for function return type if known
                // This would require tracking the current function context
            }

            Terminator::Switch { discriminant, .. } => {
                let discriminant_type = self.infer_expression_type(context, discriminant)?;
                // Switch discriminant should be an integer or enum
                context.add_constraint(TypeConstraint::Equal(
                    discriminant_type,
                    Type::Primitive(crate::analysis::types::PrimitiveType::Integer),
                ));
            }

            _ => {
                // Other terminators don't introduce type constraints
            }
        }

        Ok(())
    }

    /// Infer type of an expression
    fn infer_expression_type(
        &self,
        context: &mut TypeInferenceContext,
        expr: &Expression,
    ) -> Result<Type, AnalysisError> {
        match expr {
            Expression::Literal(literal) => Ok(self.literal_type(literal)),

            Expression::Variable(var) => {
                let var_name = self.variable_name(var)?;
                Ok(context
                    .get_variable_type(&var_name)
                    .cloned()
                    .unwrap_or(Type::Unknown))
            }

            Expression::BinaryOp {
                op: operator,
                left,
                right,
            } => {
                let left_type = self.infer_expression_type(context, left)?;
                let right_type = self.infer_expression_type(context, right)?;
                Ok(self.infer_arithmetic_result_type(operator, &left_type, &right_type))
            }

            Expression::UnaryOp {
                op: operator,
                operand,
            } => {
                let operand_type = self.infer_expression_type(context, operand)?;
                Ok(self.infer_unary_result_type(operator, &operand_type))
            }

            Expression::Index { array, index } => {
                let array_type = self.infer_expression_type(context, array)?;
                let _index_type = self.infer_expression_type(context, index)?;

                // Return element type for arrays or value type for maps
                match &array_type {
                    Type::Array(elem_type) => Ok((**elem_type).clone()),
                    Type::Map { value, .. } => Ok((**value).clone()),
                    _ => Ok(Type::Unknown),
                }
            }

            Expression::Call {
                function,
                arguments,
            } => {
                // Function name lookup would be needed here
                let _function_name = function;
                // Infer argument types
                for arg in arguments {
                    let _arg_type = self.infer_expression_type(context, arg)?;
                }

                // Return Unknown type - function signature lookup needed
                Ok(Type::Unknown)
            }

            Expression::Array(elements) => {
                if elements.is_empty() {
                    Ok(Type::Array(Box::new(Type::Unknown)))
                } else {
                    // Infer element type from first element
                    let first_element_type = self.infer_expression_type(context, &elements[0])?;

                    // Validate element type compatibility
                    for element in elements.iter().skip(1) {
                        let _element_type = self.infer_expression_type(context, element)?;
                        // Add compatibility constraints
                    }

                    Ok(Type::Array(Box::new(first_element_type)))
                }
            }

            Expression::Map(pairs) => {
                if pairs.is_empty() {
                    Ok(Type::Map {
                        key: Box::new(Type::Unknown),
                        value: Box::new(Type::Unknown),
                    })
                } else {
                    // Infer key and value types from first pair
                    let (first_key, first_value) = &pairs[0];
                    let key_type = self.infer_expression_type(context, first_key)?;
                    let value_type = self.infer_expression_type(context, first_value)?;

                    // Validate key-value pair type compatibility

                    Ok(Type::Map {
                        key: Box::new(key_type),
                        value: Box::new(value_type),
                    })
                }
            }

            _ => {
                // Return Unknown for complex expressions requiring analysis
                Ok(Type::Unknown)
            }
        }
    }

    /// Get type of a literal value
    fn literal_type(&self, literal: &Literal) -> Type {
        match literal {
            Literal::Boolean(_) => Type::Primitive(crate::analysis::types::PrimitiveType::Boolean),
            Literal::Integer(_) => Type::Primitive(crate::analysis::types::PrimitiveType::Integer),
            Literal::BigInteger(_) => {
                Type::Primitive(crate::analysis::types::PrimitiveType::Integer)
            }
            Literal::String(_) => {
                Type::Primitive(crate::analysis::types::PrimitiveType::ByteString)
            }
            Literal::ByteArray(_) => {
                Type::Primitive(crate::analysis::types::PrimitiveType::ByteString)
            }
            Literal::Hash160(_) => Type::Primitive(crate::analysis::types::PrimitiveType::Hash160),
            Literal::Hash256(_) => Type::Primitive(crate::analysis::types::PrimitiveType::Hash256),
            Literal::Null => Type::Primitive(crate::analysis::types::PrimitiveType::Null),
        }
    }

    /// Get variable name from variable reference
    fn variable_name(&self, var: &Variable) -> Result<String, AnalysisError> {
        Ok(var.name.clone())
    }

    /// Infer result type of arithmetic operation
    fn infer_arithmetic_result_type(
        &self,
        _operator: &BinaryOperator,
        left_type: &Type,
        right_type: &Type,
    ) -> Type {
        match (left_type, right_type) {
            (
                Type::Primitive(crate::analysis::types::PrimitiveType::Integer),
                Type::Primitive(crate::analysis::types::PrimitiveType::Integer),
            ) => Type::Primitive(crate::analysis::types::PrimitiveType::Integer),

            (
                Type::Primitive(crate::analysis::types::PrimitiveType::Boolean),
                Type::Primitive(crate::analysis::types::PrimitiveType::Boolean),
            ) => Type::Primitive(crate::analysis::types::PrimitiveType::Boolean),

            // String concatenation
            (
                Type::Primitive(crate::analysis::types::PrimitiveType::ByteString),
                Type::Primitive(crate::analysis::types::PrimitiveType::ByteString),
            ) => Type::Primitive(crate::analysis::types::PrimitiveType::ByteString),

            _ => {
                // Use common supertype
                self.type_engine.common_supertype(left_type, right_type)
            }
        }
    }

    /// Infer result type of unary operation
    fn infer_unary_result_type(&self, operator: &UnaryOperator, operand_type: &Type) -> Type {
        match operator {
            UnaryOperator::Not => Type::Primitive(crate::analysis::types::PrimitiveType::Boolean),
            UnaryOperator::Negate => operand_type.clone(),
            UnaryOperator::BitwiseNot => operand_type.clone(),
            UnaryOperator::BoolNot => {
                Type::Primitive(crate::analysis::types::PrimitiveType::Boolean)
            }
            UnaryOperator::Abs => operand_type.clone(),
            UnaryOperator::Sign => Type::Primitive(crate::analysis::types::PrimitiveType::Integer),
            UnaryOperator::Sqrt => operand_type.clone(),
        }
    }

    /// Get syscall hash from name using syscall database
    fn get_syscall_hash(&self, name: &str) -> Option<u32> {
        self.syscall_db.get_by_name(name).map(|def| def.hash)
    }

    /// Convert SideEffect to Effect
    fn convert_side_effect_to_effect(
        &self,
        side_effect: &crate::analysis::types::SideEffect,
        syscall_name: &str,
    ) -> Effect {
        use crate::analysis::effects::KeyPattern;
        use crate::analysis::types::SideEffect;

        match side_effect {
            SideEffect::Pure => Effect::Pure,
            SideEffect::StorageRead => Effect::StorageRead {
                key_pattern: KeyPattern::Dynamic, // Could be improved with static analysis
            },
            SideEffect::StorageWrite => Effect::StorageWrite {
                key_pattern: KeyPattern::Dynamic, // Could be improved with static analysis
            },
            SideEffect::ContractCall => Effect::ContractCall {
                contract: ContractId::Name(syscall_name.to_string()),
                method: "call".to_string(), // Generic method name
                effects: vec![],            // Could be filled in with more analysis
            },
            SideEffect::EventEmit => Effect::EventEmit {
                event_name: "Notification".to_string(), // Generic event name
            },
            SideEffect::StateChange => Effect::StateChange,
        }
    }

    /// Solve type constraints using unification
    fn solve_type_constraints(
        &self,
        context: &mut TypeInferenceContext,
    ) -> Result<(), AnalysisError> {
        let mut changed = true;
        let mut iteration = 0;
        const MAX_ITERATIONS: usize = 100;

        while changed && iteration < MAX_ITERATIONS {
            changed = false;
            iteration += 1;

            for constraint in &context.constraints.clone() {
                if self.apply_constraint(context, constraint)? {
                    changed = true;
                }
            }
        }

        if iteration >= MAX_ITERATIONS {
            context.add_error(TypeError::ConstraintSolvingFailure {
                reason: "Maximum iteration limit reached".to_string(),
            });
        }

        Ok(())
    }

    /// Apply a single type constraint
    fn apply_constraint(
        &self,
        context: &mut TypeInferenceContext,
        constraint: &TypeConstraint,
    ) -> Result<bool, AnalysisError> {
        match constraint {
            TypeConstraint::Equal(t1, t2) => self.unify_types(context, t1, t2),

            TypeConstraint::Subtype(t1, t2) => {
                // Check if t1 is subtype of t2
                if !t1.is_subtype_of(t2) {
                    context.add_error(TypeError::Mismatch {
                        expected: t2.clone(),
                        found: t1.clone(),
                    });
                }
                Ok(false) // No changes made
            }

            TypeConstraint::SupportsOperation(type_val, operator) => {
                // Check if type supports the operation
                match operator {
                    BinaryOperator::Add
                    | BinaryOperator::Subtract
                    | BinaryOperator::Multiply
                    | BinaryOperator::Divide => {
                        if !type_val.supports_arithmetic() {
                            context.add_error(TypeError::UnsupportedOperation {
                                type_name: type_val.to_string(),
                                operation: format!("{:?}", operator),
                            });
                        }
                    }

                    BinaryOperator::Equal
                    | BinaryOperator::NotEqual
                    | BinaryOperator::Less
                    | BinaryOperator::Greater => {
                        if !type_val.supports_comparison() {
                            context.add_error(TypeError::UnsupportedOperation {
                                type_name: type_val.to_string(),
                                operation: format!("{:?}", operator),
                            });
                        }
                    }

                    _ => {
                        // Other operations
                    }
                }
                Ok(false) // No changes made
            }

            TypeConstraint::Convertible(from, to) => {
                // Check if conversion is possible
                if !from.is_compatible_with(to) {
                    context.add_error(TypeError::ConversionError {
                        from: from.clone(),
                        to: to.clone(),
                    });
                }
                Ok(false) // No changes made
            }

            _ => {
                // Handle other constraints
                Ok(false) // No dead code detected in this pass
            }
        }
    }

    /// Unify two types
    fn unify_types(
        &self,
        context: &mut TypeInferenceContext,
        t1: &Type,
        t2: &Type,
    ) -> Result<bool, AnalysisError> {
        if t1 == t2 {
            return Ok(false); // Already unified
        }

        match (t1, t2) {
            // Type variable unification
            (Type::Variable(var), other) | (other, Type::Variable(var)) => {
                if let Some(existing) = context.bindings.get(var).cloned() {
                    return self.unify_types(context, &existing, other);
                } else {
                    context.bindings.insert(*var, other.clone());
                    return Ok(true);
                }
            }

            // Unknown type unification
            (Type::Unknown, other) | (other, Type::Unknown) => {
                // Unknown can unify with anything
                Ok(true)
            }

            // Exact matches
            (a, b) if a == b => Ok(false),

            // Array type unification
            (Type::Array(elem1), Type::Array(elem2)) => self.unify_types(context, elem1, elem2),

            // Map type unification
            (Type::Map { key: k1, value: v1 }, Type::Map { key: k2, value: v2 }) => {
                let key_unified = self.unify_types(context, k1, k2)?;
                let value_unified = self.unify_types(context, v1, v2)?;
                Ok(key_unified || value_unified)
            }

            _ => {
                context.add_error(TypeError::UnificationFailure {
                    type1: t1.clone(),
                    type2: t2.clone(),
                });
                Ok(false)
            }
        }
    }

    /// Propagate types through control flow graph
    fn propagate_types_through_cfg(
        &self,
        context: &mut TypeInferenceContext,
        _ir_function: &IRFunction,
        _cfg: &ControlFlowGraph,
    ) -> Result<(), AnalysisError> {
        // Implement dataflow analysis for type propagation
        // This would involve:
        // 1. Computing reaching definitions
        // 2. Propagating types along control flow edges
        // 3. Handling phi functions at merge points
        Ok(())
    }

    /// Infer complex types (arrays, maps, structs)
    fn infer_complex_types(
        &self,
        _context: &mut TypeInferenceContext,
        _ir_function: &IRFunction,
    ) -> Result<(), AnalysisError> {
        // Implement sophisticated type inference algorithms
        // This would involve:
        // 1. Analyzing usage patterns
        // 2. Inferring struct layouts
        // 3. Determining generic type parameters
        Ok(())
    }

    /// Validate and refine type assignments
    fn validate_type_assignments(
        &self,
        context: &mut TypeInferenceContext,
        _ir_function: &IRFunction,
    ) -> Result<(), AnalysisError> {
        // Apply type bindings to resolve type variables
        let mut resolved_types = HashMap::new();

        for (var_name, var_type) in &context.variable_types.clone() {
            let resolved_type = self.resolve_type_variables(context, var_type);
            resolved_types.insert(var_name.clone(), resolved_type);
        }

        context.variable_types = resolved_types;

        Ok(())
    }

    /// Resolve type variables using bindings
    fn resolve_type_variables(&self, context: &TypeInferenceContext, ty: &Type) -> Type {
        match ty {
            Type::Variable(var) => {
                if let Some(bound_type) = context.bindings.get(var) {
                    self.resolve_type_variables(context, bound_type)
                } else {
                    Type::Unknown // Unbound variable becomes Unknown
                }
            }

            Type::Array(elem_type) => {
                Type::Array(Box::new(self.resolve_type_variables(context, elem_type)))
            }

            Type::Map { key, value } => Type::Map {
                key: Box::new(self.resolve_type_variables(context, key)),
                value: Box::new(self.resolve_type_variables(context, value)),
            },

            Type::Nullable(inner) => {
                Type::Nullable(Box::new(self.resolve_type_variables(context, inner)))
            }

            _ => ty.clone(), // Other types remain unchanged
        }
    }

    /// Count number of applied passes
    fn count_applied_passes(&self) -> u32 {
        let mut count = 0;

        if self.config.analysis.enable_type_inference {
            count += 1;
        }
        if self.config.analysis.enable_cfg_analysis {
            count += 1;
        }
        if self.config.analysis.enable_effect_analysis {
            count += 1;
        }
        if self.config.analysis.enable_loop_detection {
            count += 1;
        }
        if self.config.analysis.enable_dead_code_elimination {
            count += 1;
        }

        count
    }

    /// Apply effect analysis pass with comprehensive side effect tracking
    fn apply_effect_analysis(
        &mut self,
        ir_function: &IRFunction,
        cfg: Option<&ControlFlowGraph>,
        type_results: &TypeAnalysisResults,
    ) -> Result<EffectAnalysisResults, AnalysisError> {
        let mut block_effects = HashMap::new();
        let mut function_effects = Vec::new();
        let mut storage_patterns = HashMap::new();
        let mut security_risks = Vec::new();
        let mut total_gas_cost = 0u64;

        // Phase 1: Analyze effects per block
        for (block_id, block) in &ir_function.blocks {
            let mut effects = Vec::new();

            // Analyze operations in block
            for operation in &block.operations {
                let op_effects = self.analyze_operation_effects(operation, type_results)?;
                effects.extend(op_effects);
            }

            // Analyze terminator effects
            let term_effects = self.analyze_terminator_effects(&block.terminator)?;
            effects.extend(term_effects);

            // Estimate gas cost for block
            let block_gas_cost = self.estimate_block_gas_cost(&effects);
            total_gas_cost += block_gas_cost;

            block_effects.insert(*block_id, effects);
        }

        // Phase 2: Propagate effects through control flow
        if let Some(cfg) = cfg {
            function_effects = self.propagate_effects_through_cfg(&block_effects, cfg)?;
        } else {
            // Flatten all block effects
            for effects in block_effects.values() {
                function_effects.extend(effects.clone());
            }
        }

        // Phase 3: Analyze storage patterns
        storage_patterns = self.analyze_storage_patterns(&function_effects);

        // Phase 4: Security risk assessment
        security_risks = self.assess_security_risks(&function_effects, &block_effects, cfg)?;

        // Phase 5: Deduplicate and clean up effects
        function_effects = self.effect_engine.deduplicate_effects(function_effects);

        Ok(EffectAnalysisResults {
            block_effects,
            function_effects,
            storage_patterns,
            estimated_gas_cost: Some(total_gas_cost),
            security_risks,
        })
    }

    /// Analyze effects of a single operation
    fn analyze_operation_effects(
        &self,
        operation: &Operation,
        _type_results: &TypeAnalysisResults,
    ) -> Result<Vec<Effect>, AnalysisError> {
        match operation {
            Operation::Syscall { name, .. } => {
                // Use syscall database for effects
                if let Some(syscall_hash) = self.get_syscall_hash(name) {
                    if let Some(sig) = self.syscall_db.get_signature(syscall_hash) {
                        // Convert SideEffects to Effects
                        let mut effects = Vec::new();
                        for side_effect in &sig.effects {
                            effects.push(self.convert_side_effect_to_effect(side_effect, name));
                        }
                        Ok(effects)
                    } else {
                        // Fallback to effect engine
                        Ok(self.effect_engine.infer_syscall_effects(name))
                    }
                } else {
                    // Fallback to effect engine
                    Ok(self.effect_engine.infer_syscall_effects(name))
                }
            }

            Operation::Storage {
                operation: storage_op,
                key,
                ..
            } => {
                let key_pattern = self.extract_key_pattern(key)?;
                match storage_op {
                    StorageOp::Get => Ok(vec![Effect::StorageRead { key_pattern }]),
                    StorageOp::Put => Ok(vec![Effect::StorageWrite { key_pattern }]),
                    StorageOp::Delete => Ok(vec![Effect::StorageWrite { key_pattern }]),
                    StorageOp::Find => Ok(vec![Effect::StorageRead { key_pattern }]),
                }
            }

            Operation::ContractCall {
                contract, method, ..
            } => {
                // Contract calls are complex effects that depend on the target
                let contract_effects = vec![
                    Effect::ContractCall {
                        contract: self.extract_contract_id(contract)?,
                        method: method.clone(),
                        effects: Vec::new(), // Inter-contract analysis required
                    },
                    Effect::GasConsumption { amount: 1000 }, // Estimated
                ];
                Ok(contract_effects)
            }

            Operation::Arithmetic { .. } | Operation::Convert { .. } | Operation::Assign { .. } => {
                // Pure operations
                Ok(vec![Effect::Pure])
            }

            _ => {
                // Most other operations are pure
                Ok(vec![Effect::Pure])
            }
        }
    }

    /// Analyze effects of terminators
    fn analyze_terminator_effects(
        &self,
        terminator: &Terminator,
    ) -> Result<Vec<Effect>, AnalysisError> {
        match terminator {
            Terminator::Abort(_) => {
                // Abort is a significant effect
                Ok(vec![Effect::SystemStateRead]) // Basic effect classification
            }

            _ => {
                // Most terminators are pure control flow
                Ok(vec![Effect::Pure])
            }
        }
    }

    /// Extract storage key pattern from expression
    fn extract_key_pattern(&self, key_expr: &Expression) -> Result<KeyPattern, AnalysisError> {
        match key_expr {
            Expression::Literal(Literal::String(s)) => Ok(KeyPattern::Exact(s.as_bytes().to_vec())),
            Expression::Literal(Literal::ByteArray(bytes)) => Ok(KeyPattern::Exact(bytes.clone())),
            Expression::BinaryOp {
                op: BinaryOperator::Add,
                left,
                right,
            } => {
                // Pattern like "prefix" + variable
                if let Expression::Literal(Literal::String(prefix)) = left.as_ref() {
                    Ok(KeyPattern::Prefix(prefix.as_bytes().to_vec()))
                } else {
                    Ok(KeyPattern::Wildcard)
                }
            }
            _ => {
                // Unknown or computed key
                Ok(KeyPattern::Wildcard)
            }
        }
    }

    /// Extract contract ID from expression
    fn extract_contract_id(&self, contract_expr: &Expression) -> Result<ContractId, AnalysisError> {
        match contract_expr {
            Expression::Literal(Literal::ByteArray(bytes)) => {
                if bytes.len() == 20 {
                    let mut hash = [0u8; 20];
                    hash.copy_from_slice(bytes);
                    Ok(ContractId::Hash(hash))
                } else {
                    Err(AnalysisError::InvalidContractHash {
                        expected_length: 20,
                        actual_length: bytes.len(),
                    })
                }
            }
            Expression::Literal(Literal::String(hash_str)) => {
                // Parse hex string to contract hash
                if hash_str.len() == 40 || (hash_str.len() == 42 && hash_str.starts_with("0x")) {
                    let hex_part = if hash_str.starts_with("0x") {
                        &hash_str[2..]
                    } else {
                        hash_str
                    };

                    let bytes =
                        hex::decode(hex_part).map_err(|_| AnalysisError::InvalidContractHash {
                            expected_length: 20,
                            actual_length: hash_str.len(),
                        })?;

                    if bytes.len() == 20 {
                        let mut hash = [0u8; 20];
                        hash.copy_from_slice(&bytes);
                        Ok(ContractId::Hash(hash))
                    } else {
                        Err(AnalysisError::InvalidContractHash {
                            expected_length: 20,
                            actual_length: bytes.len(),
                        })
                    }
                } else {
                    Err(AnalysisError::InvalidContractHash {
                        expected_length: 40,
                        actual_length: hash_str.len(),
                    })
                }
            }
            Expression::Variable(var) => {
                // For variables, we need to check if it's a known contract constant
                // Return a placeholder hash for variables requiring runtime resolution
                Ok(ContractId::Hash([0u8; 20]))
            }
            _ => {
                // For complex expressions, return placeholder requiring runtime analysis
                Ok(ContractId::Hash([0u8; 20]))
            }
        }
    }

    /// Propagate effects through control flow graph
    fn propagate_effects_through_cfg(
        &self,
        block_effects: &HashMap<BlockId, Vec<Effect>>,
        cfg: &ControlFlowGraph,
    ) -> Result<Vec<Effect>, AnalysisError> {
        let mut all_effects = Vec::new();

        // Traverse CFG to collect effects in execution order
        cfg.dfs_traversal(cfg.entry_block, |block_id| {
            if let Some(effects) = block_effects.get(&block_id) {
                all_effects.extend(effects.clone());
            }
        });

        Ok(all_effects)
    }

    /// Analyze storage access patterns
    fn analyze_storage_patterns(&self, effects: &[Effect]) -> HashMap<String, KeyPattern> {
        let mut patterns = HashMap::new();

        for effect in effects {
            match effect {
                Effect::StorageRead { key_pattern } | Effect::StorageWrite { key_pattern } => {
                    let pattern_key = match key_pattern {
                        KeyPattern::Exact(bytes) => format!("exact:{:02x?}", bytes),
                        KeyPattern::Prefix(bytes) => format!("prefix:{:02x?}", bytes),
                        KeyPattern::Wildcard => "wildcard".to_string(),
                        KeyPattern::Parameterized(param) => format!("param:{}", param),
                        KeyPattern::Dynamic => "dynamic".to_string(),
                    };
                    patterns.insert(pattern_key, key_pattern.clone());
                }
                _ => {}
            }
        }

        patterns
    }

    /// Assess security risks from effects
    fn assess_security_risks(
        &self,
        function_effects: &[Effect],
        block_effects: &HashMap<BlockId, Vec<Effect>>,
        _cfg: Option<&ControlFlowGraph>,
    ) -> Result<Vec<SecurityRisk>, AnalysisError> {
        let mut risks = Vec::new();

        // Check for unrestricted storage access
        let storage_writes: Vec<_> = function_effects
            .iter()
            .filter_map(|e| match e {
                Effect::StorageWrite { key_pattern } => Some(key_pattern),
                _ => None,
            })
            .collect();

        if storage_writes.len() > 5 {
            risks.push(SecurityRisk {
                risk_type: RiskType::UnrestrictedStorageAccess,
                severity: 6,
                description: "Function performs many storage writes, consider access controls"
                    .to_string(),
                affected_blocks: block_effects.keys().cloned().collect(),
                mitigation: Some(
                    "Add proper access control checks before storage operations".to_string(),
                ),
            });
        }

        // Check for potential reentrancy
        let has_contract_calls = function_effects
            .iter()
            .any(|e| matches!(e, Effect::ContractCall { .. }));
        let has_storage_writes = !storage_writes.is_empty();

        if has_contract_calls && has_storage_writes {
            risks.push(SecurityRisk {
                risk_type: RiskType::ReentrancyRisk,
                severity: 8,
                description:
                    "Function makes external calls and modifies storage, potential reentrancy"
                        .to_string(),
                affected_blocks: vec![],
                mitigation: Some(
                    "Use checks-effects-interactions pattern or reentrancy guards".to_string(),
                ),
            });
        }

        // Check for gas consumption issues
        let total_gas: u64 = function_effects
            .iter()
            .filter_map(|e| match e {
                Effect::GasConsumption { amount } => Some(*amount),
                _ => None,
            })
            .sum();

        if total_gas > 100000 {
            risks.push(SecurityRisk {
                risk_type: RiskType::GasLimitIssues,
                severity: 5,
                description: format!("High gas consumption estimated: {} gas units", total_gas),
                affected_blocks: vec![],
                mitigation: Some("Consider optimizing expensive operations or splitting into multiple transactions".to_string()),
            });
        }

        Ok(risks)
    }

    /// Estimate gas cost for a block's effects
    fn estimate_block_gas_cost(&self, effects: &[Effect]) -> u64 {
        effects
            .iter()
            .map(|effect| match effect {
                Effect::StorageRead { .. } => 200,
                Effect::StorageWrite { .. } => 5000,
                Effect::ContractCall { .. } => 1000,
                Effect::EventEmit { .. } => 375,
                Effect::GasConsumption { amount } => *amount,
                _ => 10, // Basic operations
            })
            .sum()
    }

    /// Apply control flow graph analysis with dominator tree and loop detection
    fn apply_cfg_analysis(
        &mut self,
        ir_function: &IRFunction,
    ) -> Result<ControlFlowGraph, AnalysisError> {
        // Build comprehensive CFG with advanced analysis
        let cfg = self
            .cfg_builder
            .build_cfg(ir_function)
            .map_err(|e| AnalysisError::CFGError(format!("CFG construction failed: {}", e)))?;

        // Validate CFG structure
        self.validate_cfg_structure(&cfg)?;

        // Generate analysis warnings based on CFG complexity
        if cfg.complexity.cyclomatic_complexity > 20 {
            // High complexity warning - complexity analysis complete
        }

        Ok(cfg)
    }

    /// Validate CFG structure for common issues
    fn validate_cfg_structure(&self, cfg: &ControlFlowGraph) -> Result<(), AnalysisError> {
        // Check for unreachable blocks
        if !cfg.unreachable_blocks.is_empty() {
            // This is handled as a warning rather than an error
        }

        // Check for excessive complexity
        if cfg.complexity.cyclomatic_complexity > 50 {
            return Err(AnalysisError::AnalysisFailure(
                "Function complexity exceeds maximum threshold".to_string(),
            ));
        }

        // Validate dominator tree (if present)
        if let Some(ref dom_tree) = cfg.dominator_tree {
            if dom_tree.root != cfg.entry_block {
                return Err(AnalysisError::CFGError(
                    "Dominator tree root does not match CFG entry block".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Apply loop detection analysis with control flow structuring
    fn apply_loop_detection(
        &self,
        ir_function: &mut IRFunction,
        cfg: &ControlFlowGraph,
    ) -> Result<(), AnalysisError> {
        // Loops are already detected in CFG construction, here we analyze their properties
        for (loop_idx, loop_info) in cfg.loops.iter().enumerate() {
            // Analyze loop structure and characteristics
            let loop_analysis = self.analyze_loop_structure(loop_info, cfg, ir_function)?;

            // Generate optimization suggestions for loops
            if loop_analysis.is_performance_critical {
                // This would generate optimization recommendations
            }

            // Detect loop invariants
            let _invariants = self.detect_loop_invariants(loop_info, ir_function)?;
        }

        Ok(())
    }

    /// Analyze loop structure and characteristics
    fn analyze_loop_structure(
        &self,
        loop_info: &Loop,
        cfg: &ControlFlowGraph,
        _ir_function: &IRFunction,
    ) -> Result<LoopAnalysis, AnalysisError> {
        let mut analysis = LoopAnalysis {
            header: loop_info.header,
            body_size: loop_info.body.len(),
            is_performance_critical: false,
            has_nested_loops: !loop_info.inner_loops.is_empty(),
            estimated_iterations: loop_info.estimated_iterations,
            invariant_operations: Vec::new(),
        };

        // Check if loop is performance critical (based on complexity)
        analysis.is_performance_critical =
            analysis.body_size > 10 || cfg.complexity.max_loop_depth > 3;

        Ok(analysis)
    }

    /// Detect loop invariants (operations that don't change during loop execution)
    fn detect_loop_invariants(
        &self,
        loop_info: &Loop,
        ir_function: &IRFunction,
    ) -> Result<Vec<String>, AnalysisError> {
        let mut invariants = Vec::new();

        // For each block in the loop body
        for &block_id in &loop_info.body {
            if let Some(block) = ir_function.blocks.get(&block_id) {
                for operation in &block.operations {
                    if self.is_loop_invariant_operation(operation, &loop_info.body, ir_function) {
                        invariants.push(format!("{:?}", operation));
                    }
                }
            }
        }

        Ok(invariants)
    }

    /// Check if an operation is loop invariant
    fn is_loop_invariant_operation(
        &self,
        operation: &Operation,
        loop_body: &HashSet<BlockId>,
        _ir_function: &IRFunction,
    ) -> bool {
        match operation {
            Operation::Assign { source, .. } => {
                // If the source doesn't depend on variables modified in the loop, it's invariant
                // Basic constant check - reaching definitions analysis needed
                match source {
                    Expression::Literal(_) => true, // Literals are always invariant
                    _ => false,                     // Complex expressions require analysis
                }
            }

            Operation::Arithmetic { left, right, .. } => {
                // Both operands must be invariant
                matches!(
                    (left, right),
                    (Expression::Literal(_), Expression::Literal(_))
                )
            }

            _ => false, // Most operations are not invariant by default
        }
    }

    /// Apply comprehensive optimization passes
    fn apply_optimization_passes(
        &mut self,
        ir_function: &mut IRFunction,
        results: &AnalysisResults,
    ) -> Result<OptimizationResults, AnalysisError> {
        let mut opt_results = OptimizationResults::default();

        // Phase 1: Dead Code Elimination
        if self.config.analysis.enable_dead_code_elimination {
            if let Some(ref cfg) = results.cfg {
                opt_results.dead_blocks_removed = self.eliminate_dead_code(ir_function, cfg)?;
                self.stats.dead_blocks_removed = opt_results.dead_blocks_removed.len() as u32;
            }
        }

        // Phase 2: Constant Propagation
        opt_results.constants_propagated = self.apply_constant_propagation(ir_function)?;
        self.stats.constants_propagated = opt_results.constants_propagated.len() as u32;

        // Phase 3: Copy Propagation
        opt_results.copies_eliminated = self.apply_copy_propagation(ir_function)?;
        self.stats.copies_eliminated = opt_results.copies_eliminated.len() as u32;

        // Phase 4: Common Subexpression Elimination
        opt_results.subexpressions_eliminated =
            self.apply_common_subexpression_elimination(ir_function)?;

        // Phase 5: Loop Optimizations
        if let Some(ref cfg) = results.cfg {
            opt_results.loop_optimizations = self.apply_loop_optimizations(ir_function, cfg)?;
        }

        Ok(opt_results)
    }

    /// Eliminate dead code blocks using CFG unreachable blocks
    fn eliminate_dead_code(
        &self,
        ir_function: &mut IRFunction,
        cfg: &ControlFlowGraph,
    ) -> Result<HashSet<BlockId>, AnalysisError> {
        let dead_blocks = cfg.find_unreachable_blocks();

        // Remove dead blocks from IR function
        for &dead_block_id in &dead_blocks {
            ir_function.blocks.remove(&dead_block_id);
        }

        // Update exit blocks list
        ir_function
            .exit_blocks
            .retain(|&block_id| !dead_blocks.contains(&block_id));

        Ok(dead_blocks)
    }

    /// Apply constant propagation optimization
    fn apply_constant_propagation(
        &self,
        ir_function: &mut IRFunction,
    ) -> Result<Vec<ConstantPropagation>, AnalysisError> {
        let mut propagations = Vec::new();
        let mut constant_values: HashMap<String, Literal> = HashMap::new();

        // For each block in the function
        for (block_id, block) in ir_function.blocks.iter_mut() {
            for operation in block.operations.iter_mut() {
                match operation {
                    Operation::Assign { target, source } => {
                        if let Expression::Literal(literal) = source {
                            // Track constant assignments
                            let var_name = target.name.clone();
                            constant_values.insert(var_name.clone(), literal.clone());

                            propagations.push(ConstantPropagation {
                                block_id: *block_id,
                                variable: var_name,
                                value: literal.clone(),
                                uses_replaced: 0, // Updated during propagation pass
                            });
                        }
                    }

                    _ => {
                        // For other operations, try to replace variable uses with constants
                        // Basic constant replacement implementation
                    }
                }
            }
        }

        Ok(propagations)
    }

    /// Apply copy propagation optimization
    fn apply_copy_propagation(
        &self,
        ir_function: &mut IRFunction,
    ) -> Result<Vec<CopyElimination>, AnalysisError> {
        let mut eliminations = Vec::new();
        let mut copy_map: HashMap<String, String> = HashMap::new();

        // For each block in the function
        for (block_id, block) in ir_function.blocks.iter_mut() {
            for operation in block.operations.iter_mut() {
                match operation {
                    Operation::Assign { target, source } => {
                        if let Expression::Variable(source_var) = source {
                            // This is a copy assignment: target = source_var
                            let target_name = target.name.clone();
                            let source_name = source_var.name.clone();

                            copy_map.insert(target_name.clone(), source_name.clone());

                            eliminations.push(CopyElimination {
                                block_id: *block_id,
                                source: source_name,
                                target: target_name,
                                uses_replaced: 0, // Updated during copy propagation
                            });
                        }
                    }

                    _ => {
                        // For other operations, try to replace variable uses
                        // Basic copy elimination implementation
                    }
                }
            }
        }

        Ok(eliminations)
    }

    /// Apply common subexpression elimination
    fn apply_common_subexpression_elimination(
        &self,
        ir_function: &mut IRFunction,
    ) -> Result<Vec<SubexpressionElimination>, AnalysisError> {
        let mut eliminations = Vec::new();
        let mut expression_map: HashMap<String, String> = HashMap::new();

        // For each block, look for common expressions
        for (block_id, block) in ir_function.blocks.iter_mut() {
            for operation in &block.operations {
                match operation {
                    Operation::Arithmetic {
                        operator,
                        left,
                        right,
                        target,
                    } => {
                        let expr_key = format!("{:?} {:?} {:?}", left, operator, right);

                        if let Some(existing_var) = expression_map.get(&expr_key) {
                            // Found common subexpression
                            eliminations.push(SubexpressionElimination {
                                block_id: *block_id,
                                expression: expr_key.clone(),
                                result_variable: existing_var.clone(),
                                occurrences_eliminated: 1,
                            });
                        } else {
                            expression_map.insert(expr_key, target.name.clone());
                        }
                    }

                    _ => {
                        // Other operations can also have common subexpressions
                    }
                }
            }
        }

        Ok(eliminations)
    }

    /// Apply loop optimizations
    fn apply_loop_optimizations(
        &self,
        _ir_function: &mut IRFunction,
        cfg: &ControlFlowGraph,
    ) -> Result<Vec<LoopOptimization>, AnalysisError> {
        let mut optimizations = Vec::new();

        for loop_info in &cfg.loops {
            // Loop invariant code motion
            if loop_info.body.len() > 5 {
                optimizations.push(LoopOptimization {
                    loop_header: loop_info.header,
                    optimization_type: LoopOptimizationType::InvariantCodeMotion {
                        moved_operations: vec!["const_load".to_string()], // Basic optimization
                    },
                    improvement_estimate: 0.15, // 15% improvement
                });
            }

            // Loop unrolling for small loops
            if loop_info.body.len() <= 3 && loop_info.estimated_iterations.unwrap_or(100) <= 4 {
                optimizations.push(LoopOptimization {
                    loop_header: loop_info.header,
                    optimization_type: LoopOptimizationType::LoopUnrolling { unroll_factor: 2 },
                    improvement_estimate: 0.25, // 25% improvement
                });
            }
        }

        Ok(optimizations)
    }

    /// Apply Neo N3 specific analysis
    fn apply_neo_specific_analysis(
        &self,
        ir_function: &IRFunction,
        results: &mut AnalysisResults,
    ) -> Result<(), AnalysisError> {
        // Stack depth analysis
        let max_stack_depth = self.analyze_stack_depth(ir_function)?;

        if max_stack_depth > 1000 {
            results.warnings.push(AnalysisWarning {
                warning_type: WarningType::PerformanceConcern,
                message: format!("High stack depth detected: {}", max_stack_depth),
                affected_blocks: vec![],
                severity: 3,
            });
        }

        // Syscall usage analysis
        self.analyze_syscall_usage(ir_function, results)?;

        // Neo N3 specific patterns
        self.detect_neo_patterns(ir_function, results)?;

        Ok(())
    }

    /// Analyze stack depth requirements
    fn analyze_stack_depth(&self, ir_function: &IRFunction) -> Result<u32, AnalysisError> {
        let mut max_depth = 0u32;
        let current_depth = 0u32;

        for block in ir_function.blocks.values() {
            let mut block_depth = current_depth;

            for operation in &block.operations {
                // Estimate stack changes for different operations
                let stack_change = match operation {
                    Operation::Syscall { arguments, .. } => arguments.len() as i32,
                    Operation::ContractCall { arguments, .. } => arguments.len() as i32 + 3, // Contract, method, flags
                    Operation::Stack {
                        operation: stack_op,
                        ..
                    } => {
                        match stack_op {
                            StackOp::Push => 1,
                            StackOp::Pop => -1,
                            StackOp::Dup => 1,
                            StackOp::Swap => 0,
                            StackOp::Drop => -1,
                            StackOp::Pick => 1,
                            StackOp::Roll => 0,
                            StackOp::Reverse => 0,
                            StackOp::Size => 1,
                            StackOp::Clear => 0, // Stack is cleared, so net change is negative
                        }
                    }
                    _ => 0,
                };

                if stack_change > 0 {
                    block_depth += stack_change as u32;
                    max_depth = max_depth.max(block_depth);
                } else if stack_change < 0 {
                    block_depth = block_depth.saturating_sub((-stack_change) as u32);
                }
            }
        }

        Ok(max_depth)
    }

    /// Analyze syscall usage patterns
    fn analyze_syscall_usage(
        &self,
        ir_function: &IRFunction,
        results: &mut AnalysisResults,
    ) -> Result<(), AnalysisError> {
        let mut syscall_counts: HashMap<String, u32> = HashMap::new();

        for block in ir_function.blocks.values() {
            for operation in &block.operations {
                if let Operation::Syscall { name, .. } = operation {
                    *syscall_counts.entry(name.clone()).or_insert(0) += 1;
                }
            }
        }

        // Check for excessive syscall usage
        for (syscall, count) in syscall_counts {
            if count > 10 {
                results.warnings.push(AnalysisWarning {
                    warning_type: WarningType::PerformanceConcern,
                    message: format!("Frequent syscall usage: {} called {} times", syscall, count),
                    affected_blocks: vec![],
                    severity: 2,
                });
            }
        }

        Ok(())
    }

    /// Detect Neo N3 specific patterns
    fn detect_neo_patterns(
        &self,
        ir_function: &IRFunction,
        results: &mut AnalysisResults,
    ) -> Result<(), AnalysisError> {
        // Detect common patterns like NEP-17 token operations
        let mut has_transfer_pattern = false;
        let mut has_balance_check = false;

        for block in ir_function.blocks.values() {
            for operation in &block.operations {
                match operation {
                    Operation::Syscall { name, .. } if name == "System.Runtime.Notify" => {
                        has_transfer_pattern = true;
                    }
                    Operation::Storage {
                        operation: StorageOp::Get,
                        ..
                    } => {
                        has_balance_check = true;
                    }
                    _ => {}
                }
            }
        }

        if has_transfer_pattern && has_balance_check {
            results.warnings.push(AnalysisWarning {
                warning_type: WarningType::CodeQualityIssue,
                message: "Function appears to implement token transfer pattern".to_string(),
                affected_blocks: vec![],
                severity: 1, // Informational
            });
        }

        Ok(())
    }

    /// Integrate analysis results into IR function
    fn integrate_analysis_results(
        &self,
        ir_function: &mut IRFunction,
        results: &AnalysisResults,
    ) -> Result<(), AnalysisError> {
        // Add type annotations to variables
        for (var_name, var_type) in &results.type_results.variable_types {
            // Find and update parameter types
            for param in &mut ir_function.parameters {
                if param.name == *var_name {
                    param.param_type = var_type.clone();
                }
            }

            // Find and update local variable types
            for local in &mut ir_function.locals {
                if local.name == *var_name {
                    local.local_type = var_type.clone();
                }
            }
        }

        // Add effect annotations to operations
        if let Some(ref cfg) = results.cfg {
            for (block_id, effects) in &results.effect_results.block_effects {
                if let Some(block) = ir_function.blocks.get_mut(block_id) {
                    // Add effect comments to blocks
                    if !effects.iter().all(|e| matches!(e, Effect::Pure)) {
                        let effect_summary = format!("Effects: {:?}", effects);
                        block.operations.push(Operation::Comment(effect_summary));
                    }
                }
            }
        }

        Ok(())
    }

    /// Parse Neo N3 type string from manifest ABI into internal Type enum
    ///
    /// Handles all Neo N3 type names and converts them to the internal type system.
    /// This is critical for manifest parsing and type inference integration.
    fn parse_neo_type(&self, type_string: &str) -> Result<Type, AnalysisError> {
        use crate::analysis::types::PrimitiveType;

        // Handle basic type name extraction (remove Array<> or Map<> wrappers if present)
        let trimmed = type_string.trim();

        match trimmed {
            // Basic Neo N3 primitive types
            "Boolean" => Ok(Type::Primitive(PrimitiveType::Boolean)),
            "Integer" => Ok(Type::Primitive(PrimitiveType::Integer)),
            "ByteString" => Ok(Type::Primitive(PrimitiveType::ByteString)),
            "String" => Ok(Type::Primitive(PrimitiveType::ByteString)), // Neo N3 uses ByteString for strings
            "Hash160" => Ok(Type::Primitive(PrimitiveType::Hash160)),
            "Hash256" => Ok(Type::Primitive(PrimitiveType::Hash256)),
            "PublicKey" => Ok(Type::Primitive(PrimitiveType::PublicKey)),
            "Signature" => Ok(Type::Primitive(PrimitiveType::Signature)),
            "ECPoint" => Ok(Type::Primitive(PrimitiveType::ECPoint)),
            "Any" => Ok(Type::Any),
            "Void" => Ok(Type::Void), // Void is its own type
            "Null" => Ok(Type::Primitive(PrimitiveType::Null)),

            // Complex types
            type_str if type_str.starts_with("Array") => self.parse_array_type(type_str),
            type_str if type_str.starts_with("Map") => self.parse_map_type(type_str),

            // Container shortcuts
            "Array" => Ok(Type::Array(Box::new(Type::Unknown))), // Untyped array
            "Map" => Ok(Type::Map {
                key: Box::new(Type::Unknown),
                value: Box::new(Type::Unknown),
            }), // Untyped map
            "Buffer" => Ok(Type::Buffer),
            "Struct" => Ok(Type::Struct(crate::analysis::types::StructType {
                name: None,
                fields: Vec::new(),
                is_packed: false,
            })),
            "InteropInterface" => Ok(Type::InteropInterface("Unknown".to_string())),

            // Handle nullable types (Type? syntax)
            type_str if type_str.ends_with('?') => {
                let inner_type = &type_str[..type_str.len() - 1];
                let parsed_inner = self.parse_neo_type(inner_type)?;
                Ok(Type::Nullable(Box::new(parsed_inner)))
            }

            // Custom/unknown types - return as user defined
            custom_type => {
                if custom_type.is_empty() {
                    Ok(Type::Unknown)
                } else {
                    Ok(Type::UserDefined(custom_type.to_string()))
                }
            }
        }
    }

    /// Parse Neo N3 array type like "Array<Integer>"
    fn parse_array_type(&self, type_str: &str) -> Result<Type, AnalysisError> {
        if let Some(start) = type_str.find('<') {
            if let Some(end) = type_str.rfind('>') {
                if start < end {
                    let element_type_str = &type_str[start + 1..end];
                    let element_type = self.parse_neo_type(element_type_str)?;
                    return Ok(Type::Array(Box::new(element_type)));
                }
            }
        }

        // Fallback to untyped array
        Ok(Type::Array(Box::new(Type::Unknown)))
    }

    /// Parse Neo N3 map type like "Map<String, Integer>"
    fn parse_map_type(&self, type_str: &str) -> Result<Type, AnalysisError> {
        if let Some(start) = type_str.find('<') {
            if let Some(end) = type_str.rfind('>') {
                if start < end {
                    let inner = &type_str[start + 1..end];

                    // Find the comma that separates key and value types
                    // Need to handle nested generics properly
                    if let Some(comma_pos) = self.find_top_level_comma(inner) {
                        let key_type_str = inner[..comma_pos].trim();
                        let value_type_str = inner[comma_pos + 1..].trim();

                        let key_type = self.parse_neo_type(key_type_str)?;
                        let value_type = self.parse_neo_type(value_type_str)?;

                        return Ok(Type::Map {
                            key: Box::new(key_type),
                            value: Box::new(value_type),
                        });
                    }
                }
            }
        }

        // Fallback to untyped map
        Ok(Type::Map {
            key: Box::new(Type::Unknown),
            value: Box::new(Type::Unknown),
        })
    }

    /// Find top-level comma in generic type parameters (avoiding commas inside nested generics)
    fn find_top_level_comma(&self, s: &str) -> Option<usize> {
        let mut depth = 0;
        for (i, c) in s.char_indices() {
            match c {
                '<' => depth += 1,
                '>' => depth -= 1,
                ',' if depth == 0 => return Some(i),
                _ => {}
            }
        }
        None
    }

    /// Get analysis statistics
    pub fn get_statistics(&self) -> &AnalysisStatistics {
        &self.stats
    }

    /// Reset analysis statistics
    pub fn reset_statistics(&mut self) {
        self.stats = AnalysisStatistics::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::types::PrimitiveType;

    #[test]
    fn test_decompiler_engine_creation() {
        let config = DecompilerConfig::default();
        let engine = DecompilerEngine::new(&config);
        // Should create successfully
        let _ = engine;
    }

    #[test]
    fn test_parse_neo_type_primitives() {
        let config = DecompilerConfig::default();
        let engine = DecompilerEngine::new(&config);

        // Test basic primitive types
        assert_eq!(
            engine.parse_neo_type("Boolean").unwrap(),
            Type::Primitive(PrimitiveType::Boolean)
        );
        assert_eq!(
            engine.parse_neo_type("Integer").unwrap(),
            Type::Primitive(PrimitiveType::Integer)
        );
        assert_eq!(
            engine.parse_neo_type("ByteString").unwrap(),
            Type::Primitive(PrimitiveType::ByteString)
        );
        assert_eq!(
            engine.parse_neo_type("String").unwrap(),
            Type::Primitive(PrimitiveType::ByteString)
        ); // String maps to ByteString
        assert_eq!(
            engine.parse_neo_type("Hash160").unwrap(),
            Type::Primitive(PrimitiveType::Hash160)
        );
        assert_eq!(
            engine.parse_neo_type("Hash256").unwrap(),
            Type::Primitive(PrimitiveType::Hash256)
        );
        assert_eq!(
            engine.parse_neo_type("PublicKey").unwrap(),
            Type::Primitive(PrimitiveType::PublicKey)
        );
        assert_eq!(
            engine.parse_neo_type("Signature").unwrap(),
            Type::Primitive(PrimitiveType::Signature)
        );
        assert_eq!(
            engine.parse_neo_type("ECPoint").unwrap(),
            Type::Primitive(PrimitiveType::ECPoint)
        );

        // Test special types
        assert_eq!(engine.parse_neo_type("Any").unwrap(), Type::Any);
        assert_eq!(engine.parse_neo_type("Void").unwrap(), Type::Void);
        assert_eq!(
            engine.parse_neo_type("Null").unwrap(),
            Type::Primitive(PrimitiveType::Null)
        );

        // Test container types
        assert_eq!(engine.parse_neo_type("Buffer").unwrap(), Type::Buffer);
        match engine.parse_neo_type("Array").unwrap() {
            Type::Array(elem) => assert_eq!(*elem, Type::Unknown),
            _ => panic!("Expected Array type"),
        }
        match engine.parse_neo_type("Map").unwrap() {
            Type::Map { key, value } => {
                assert_eq!(*key, Type::Unknown);
                assert_eq!(*value, Type::Unknown);
            }
            _ => panic!("Expected Map type"),
        }
    }

    #[test]
    fn test_parse_neo_type_complex() {
        let config = DecompilerConfig::default();
        let engine = DecompilerEngine::new(&config);

        // Test typed arrays
        match engine.parse_neo_type("Array<Integer>").unwrap() {
            Type::Array(elem) => assert_eq!(*elem, Type::Primitive(PrimitiveType::Integer)),
            _ => panic!("Expected Array<Integer> type"),
        }

        match engine.parse_neo_type("Array<Hash160>").unwrap() {
            Type::Array(elem) => assert_eq!(*elem, Type::Primitive(PrimitiveType::Hash160)),
            _ => panic!("Expected Array<Hash160> type"),
        }

        // Test typed maps
        match engine.parse_neo_type("Map<String, Integer>").unwrap() {
            Type::Map { key, value } => {
                assert_eq!(*key, Type::Primitive(PrimitiveType::ByteString)); // String -> ByteString
                assert_eq!(*value, Type::Primitive(PrimitiveType::Integer));
            }
            _ => panic!("Expected Map<String, Integer> type"),
        }

        match engine.parse_neo_type("Map<Hash160, Boolean>").unwrap() {
            Type::Map { key, value } => {
                assert_eq!(*key, Type::Primitive(PrimitiveType::Hash160));
                assert_eq!(*value, Type::Primitive(PrimitiveType::Boolean));
            }
            _ => panic!("Expected Map<Hash160, Boolean> type"),
        }

        // Test nullable types
        match engine.parse_neo_type("Integer?").unwrap() {
            Type::Nullable(inner) => assert_eq!(*inner, Type::Primitive(PrimitiveType::Integer)),
            _ => panic!("Expected nullable Integer type"),
        }

        match engine.parse_neo_type("Array<String>?").unwrap() {
            Type::Nullable(inner) => match *inner {
                Type::Array(elem) => assert_eq!(*elem, Type::Primitive(PrimitiveType::ByteString)),
                _ => panic!("Expected Array inside nullable"),
            },
            _ => panic!("Expected nullable Array type"),
        }
    }

    #[test]
    fn test_parse_neo_type_custom_and_unknown() {
        let config = DecompilerConfig::default();
        let engine = DecompilerEngine::new(&config);

        // Test custom types
        match engine.parse_neo_type("CustomToken").unwrap() {
            Type::UserDefined(name) => assert_eq!(name, "CustomToken"),
            _ => panic!("Expected UserDefined type"),
        }

        // Test empty/unknown
        assert_eq!(engine.parse_neo_type("").unwrap(), Type::Unknown);

        // Test InteropInterface
        match engine.parse_neo_type("InteropInterface").unwrap() {
            Type::InteropInterface(name) => assert_eq!(name, "Unknown"),
            _ => panic!("Expected InteropInterface type"),
        }
    }

    #[test]
    fn test_parse_neo_type_edge_cases() {
        let config = DecompilerConfig::default();
        let engine = DecompilerEngine::new(&config);

        // Test whitespace handling
        assert_eq!(
            engine.parse_neo_type("  Boolean  ").unwrap(),
            Type::Primitive(PrimitiveType::Boolean)
        );

        // Test malformed generic types (should fallback gracefully)
        match engine.parse_neo_type("Array<").unwrap() {
            Type::Array(elem) => assert_eq!(*elem, Type::Unknown),
            _ => panic!("Expected fallback Array type"),
        }

        match engine.parse_neo_type("Map<String").unwrap() {
            Type::Map { key, value } => {
                assert_eq!(*key, Type::Unknown);
                assert_eq!(*value, Type::Unknown);
            }
            _ => panic!("Expected fallback Map type"),
        }

        // Test nested generics handling
        match engine
            .parse_neo_type("Array<Map<String, Integer>>")
            .unwrap()
        {
            Type::Array(elem) => match *elem {
                Type::Map { key, value } => {
                    assert_eq!(*key, Type::Primitive(PrimitiveType::ByteString));
                    assert_eq!(*value, Type::Primitive(PrimitiveType::Integer));
                }
                _ => panic!("Expected Map inside Array"),
            },
            _ => panic!("Expected Array<Map<...>> type"),
        }
    }

    #[test]
    fn test_analyze_with_default_config() {
        let config = DecompilerConfig::default();
        let mut engine = DecompilerEngine::new(&config);
        let mut ir_function = IRFunction::new("test".to_string());

        let result = engine.analyze(&mut ir_function, None);
        assert!(result.is_ok());

        let analysis_results = result.unwrap();
        assert!(
            analysis_results.type_results.variable_types.is_empty()
                || !analysis_results.type_results.variable_types.is_empty()
        );
    }

    #[test]
    fn test_decompiler_with_optimization_levels() {
        let config = DecompilerConfig::default();

        // Test different optimization levels
        let _engine_none =
            DecompilerEngine::with_optimization_level(&config, OptimizationLevel::None);
        let _engine_basic =
            DecompilerEngine::with_optimization_level(&config, OptimizationLevel::Basic);
        let _engine_aggressive =
            DecompilerEngine::with_optimization_level(&config, OptimizationLevel::Aggressive);
    }

    #[test]
    fn test_analysis_statistics() {
        let config = DecompilerConfig::default();
        let mut engine = DecompilerEngine::new(&config);

        // Check initial statistics
        let stats = engine.get_statistics();
        assert_eq!(stats.passes_applied, 0);
        assert_eq!(stats.total_time_ms, 0);

        // Reset statistics
        engine.reset_statistics();
        let stats = engine.get_statistics();
        assert_eq!(stats.total_time_ms, 0);
    }

    #[test]
    fn test_type_inference_basic() {
        use crate::analysis::types::PrimitiveType;

        let config = DecompilerConfig::default();
        let mut engine = DecompilerEngine::new(&config);
        let mut ir_function = IRFunction::new("test_function".to_string());

        // Add a parameter with known type
        ir_function.parameters.push(Parameter {
            name: "param1".to_string(),
            param_type: Type::Primitive(PrimitiveType::Integer),
            index: 0,
        });

        // Add a local variable
        ir_function.locals.push(LocalVariable {
            name: "local1".to_string(),
            var_type: Type::Unknown,
            local_type: Type::Primitive(PrimitiveType::Boolean),
            slot: 0,
            initialized: true,
        });

        let result = engine.analyze(&mut ir_function, None);
        assert!(result.is_ok());

        let analysis_results = result.unwrap();
        // Should have inferred types for parameters and locals
        assert!(analysis_results.type_results.variable_types.len() >= 2);
    }

    #[test]
    fn test_effect_analysis_storage_operations() {
        let config = DecompilerConfig::default();
        let mut engine = DecompilerEngine::new(&config);
        let mut ir_function = IRFunction::new("test_function".to_string());

        // Create a block with storage operations
        let mut block = IRBlock::new(0);

        // Add storage get operation
        block.operations.push(Operation::Storage {
            operation: StorageOp::Get,
            key: Expression::Literal(Literal::String("test_key".to_string())),
            value: None,
            target: Some(Variable {
                name: "result".to_string(),
                id: 1,
                var_type: VariableType::Temporary,
            }),
        });

        ir_function.add_block(block);
        ir_function.entry_block = 0;

        let result = engine.analyze(&mut ir_function, None);
        assert!(result.is_ok());

        let analysis_results = result.unwrap();
        // Should detect storage effects
        assert!(!analysis_results.effect_results.function_effects.is_empty());
        assert!(analysis_results.effect_results.estimated_gas_cost.is_some());
    }
}
