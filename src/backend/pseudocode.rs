//! Advanced pseudocode generation from IR with multiple syntax styles and comprehensive features
//!
//! This module provides sophisticated pseudocode generation capabilities for Neo N3 smart contracts,
//! supporting multiple output syntax styles, advanced code structuring, and security-focused audit features.

use crate::analysis::types::{PrimitiveType, Type};
use crate::common::{
    config::{DecompilerConfig, SyntaxStyle},
    errors::CodeGenerationError,
    types::*,
};
use crate::core::ir::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Write;

/// Advanced pseudocode generator with multiple syntax support
pub struct PseudocodeGenerator {
    /// Configuration
    config: DecompilerConfig,
    /// Syscall hash to name mapping
    syscall_names: HashMap<u32, String>,
    /// Current indentation level
    current_indent: usize,
    /// Variable name mapping for intelligent naming
    variable_names: HashMap<String, String>,
    /// Generated variable counter
    var_counter: u32,
    /// Loop detection state
    detected_loops: HashSet<BlockId>,
    /// Conditional structure state
    conditional_stack: Vec<ConditionalContext>,
    /// Current syntax style formatter
    formatter: Box<dyn SyntaxFormatter>,
}

/// Context for tracking conditional structures during generation
#[derive(Debug, Clone)]
struct ConditionalContext {
    condition_type: ConditionType,
    true_target: BlockId,
    false_target: Option<BlockId>,
    merged_target: Option<BlockId>,
}

/// Types of conditional structures
#[derive(Debug, Clone, PartialEq)]
enum ConditionType {
    If,
    ElseIf,
    While,
    Switch,
}

/// Trait for different syntax formatting styles
trait SyntaxFormatter {
    fn format_function_signature(
        &self,
        function: &IRFunction,
        generator: &PseudocodeGenerator,
    ) -> String;
    fn format_block_start(&self, block_id: BlockId) -> String;
    fn format_block_end(&self) -> String;
    fn format_statement(&self, statement: &str) -> String;
    fn format_assignment(&self, target: &str, source: &str) -> String;
    fn format_function_call(
        &self,
        name: &str,
        args: &[String],
        return_type: Option<&Type>,
    ) -> String;
    fn format_if_statement(&self, condition: &str, generator: &PseudocodeGenerator) -> String;
    fn format_while_loop(&self, condition: &str) -> String;
    fn format_return(&self, expr: Option<&str>) -> String;
    fn format_type_annotation(&self, var_name: &str, type_name: &str) -> String;
    fn format_comment(&self, text: &str) -> String;
    fn format_security_annotation(&self, annotation: &str) -> String;
    fn get_line_ending(&self) -> &'static str;
    fn get_block_start(&self) -> &'static str;
    fn get_block_end(&self) -> &'static str;
}

impl PseudocodeGenerator {
    /// Create new pseudocode generator with advanced features
    pub fn new(config: &DecompilerConfig) -> Self {
        let formatter: Box<dyn SyntaxFormatter> = match config.output.syntax_style {
            SyntaxStyle::CStyle => Box::new(CStyleFormatter),
            SyntaxStyle::Python => Box::new(PythonFormatter),
            SyntaxStyle::Rust => Box::new(RustFormatter),
            SyntaxStyle::TypeScript => Box::new(TypeScriptFormatter),
        };

        let mut generator = Self {
            config: config.clone(),
            syscall_names: HashMap::new(),
            current_indent: 0,
            variable_names: HashMap::new(),
            var_counter: 0,
            detected_loops: HashSet::new(),
            conditional_stack: Vec::new(),
            formatter,
        };

        generator.initialize_syscall_names();
        generator
    }

    /// Initialize syscall hash to name mapping
    fn initialize_syscall_names(&mut self) {
        // Load syscall database from configuration
        self.syscall_names
            .insert(0x49de7d57, "System.Runtime.Platform".to_string());
        self.syscall_names
            .insert(0x2d43a8aa, "System.Runtime.GetTrigger".to_string());
        self.syscall_names
            .insert(0xf827ec8e, "System.Runtime.GetTime".to_string());
        self.syscall_names.insert(
            0x5d97c1b2,
            "System.Runtime.GetExecutingScriptHash".to_string(),
        );
        self.syscall_names.insert(
            0x91f9b23b,
            "System.Runtime.GetCallingScriptHash".to_string(),
        );
        self.syscall_names
            .insert(0x9e29b9a8, "System.Runtime.GetEntryScriptHash".to_string());
        self.syscall_names
            .insert(0x9c7c9598, "System.Storage.GetContext".to_string());
        self.syscall_names
            .insert(0xe1c83c39, "System.Storage.GetReadOnlyContext".to_string());
        self.syscall_names
            .insert(0x925de831, "System.Storage.Get".to_string());
        self.syscall_names
            .insert(0xe63f1884, "System.Storage.Put".to_string());
        self.syscall_names
            .insert(0x7ce2e494, "System.Storage.Delete".to_string());
        self.syscall_names
            .insert(0xa09b1eef, "System.Storage.Find".to_string());
        self.syscall_names
            .insert(0x627d5b52, "System.Contract.Call".to_string());
        self.syscall_names
            .insert(0x14e12327, "System.Contract.CallEx".to_string());
        self.syscall_names
            .insert(0x82958f5a, "System.Crypto.CheckSig".to_string());
        self.syscall_names
            .insert(0xf60652e8, "System.Crypto.CheckMultisig".to_string());
        self.syscall_names
            .insert(0x7e6a2bb7, "System.Iterator.Next".to_string());
        self.syscall_names
            .insert(0x63b6c5ee, "System.Iterator.Value".to_string());
        self.syscall_names
            .insert(0xa0ab5461, "System.Json.Serialize".to_string());
        self.syscall_names
            .insert(0x7d4b2a25, "System.Json.Deserialize".to_string());
    }

    /// Generate advanced pseudocode from IR function with structure recovery
    pub fn generate(&mut self, function: &IRFunction) -> Result<String, CodeGenerationError> {
        let mut output = String::new();

        // Reset generator state
        self.current_indent = 0;
        self.variable_names.clear();
        self.var_counter = 0;
        self.detected_loops.clear();
        self.conditional_stack.clear();

        // Perform structure analysis
        self.analyze_control_flow(function)?;

        // Generate function header with metadata
        if self.config.output.include_ir_comments {
            output.push_str(&self.format_comment(&format!(
                "Function: {} (Complexity: cyclomatic={}, blocks={}, operations={})",
                function.name,
                function.metadata.complexity.cyclomatic,
                function.metadata.complexity.blocks,
                function.metadata.complexity.operations
            )));
            output.push('\n');

            // Add security annotations if function has notable characteristics
            if function.metadata.is_public {
                output.push_str(
                    &self
                        .formatter
                        .format_security_annotation("@public - External entry point"),
                );
                output.push('\n');
            }
            if function.metadata.is_safe {
                output.push_str(
                    &self
                        .formatter
                        .format_security_annotation("@safe - Read-only function"),
                );
                output.push('\n');
            }
        }

        // Generate function signature
        output.push_str(&self.formatter.format_function_signature(function, self));
        output.push_str(self.formatter.get_block_start());
        output.push('\n');

        self.current_indent += 1;

        // Generate function body with structured control flow
        let generated_blocks = self.generate_structured_blocks(function, function.entry_block)?;
        output.push_str(&generated_blocks);

        self.current_indent -= 1;
        output.push_str(self.formatter.get_block_end());
        output.push('\n');

        Ok(output)
    }

    /// Analyze control flow patterns to detect loops and structured conditionals
    fn analyze_control_flow(&mut self, function: &IRFunction) -> Result<(), CodeGenerationError> {
        // Simple back-edge detection for loops
        let mut visited = HashSet::new();
        let mut in_stack = HashSet::new();

        fn dfs_visit(
            block_id: BlockId,
            function: &IRFunction,
            visited: &mut HashSet<BlockId>,
            in_stack: &mut HashSet<BlockId>,
            detected_loops: &mut HashSet<BlockId>,
        ) -> Result<(), CodeGenerationError> {
            visited.insert(block_id);
            in_stack.insert(block_id);

            if let Some(block) = function.get_block(block_id) {
                // Get successor blocks based on terminator
                let successors = match &block.terminator {
                    Terminator::Jump(target) => vec![*target],
                    Terminator::Branch {
                        true_target,
                        false_target,
                        ..
                    } => vec![*true_target, *false_target],
                    Terminator::Switch {
                        targets,
                        default_target,
                        ..
                    } => {
                        let mut succ = targets
                            .iter()
                            .map(|(_, target)| *target)
                            .collect::<Vec<_>>();
                        if let Some(default) = default_target {
                            succ.push(*default);
                        }
                        succ
                    }
                    _ => vec![],
                };

                for successor in successors {
                    if in_stack.contains(&successor) {
                        // Back edge found - indicates loop
                        detected_loops.insert(successor);
                    } else if !visited.contains(&successor) {
                        dfs_visit(successor, function, visited, in_stack, detected_loops)?;
                    }
                }
            }

            in_stack.remove(&block_id);
            Ok(())
        }

        dfs_visit(
            function.entry_block,
            function,
            &mut visited,
            &mut in_stack,
            &mut self.detected_loops,
        )?;
        Ok(())
    }

    /// Generate structured control flow blocks with proper nesting
    fn generate_structured_blocks(
        &mut self,
        function: &IRFunction,
        start_block: BlockId,
    ) -> Result<String, CodeGenerationError> {
        let mut output = String::new();
        let mut visited = HashSet::new();
        let mut work_queue = VecDeque::new();
        work_queue.push_back(start_block);

        while let Some(block_id) = work_queue.pop_front() {
            if visited.contains(&block_id) {
                continue;
            }
            visited.insert(block_id);

            if let Some(block) = function.get_block(block_id) {
                let block_code = self.generate_block_with_structure(block, function)?;
                output.push_str(&block_code);

                // Add successor blocks to queue based on terminator
                match &block.terminator {
                    Terminator::Jump(target) => {
                        if !visited.contains(target) {
                            work_queue.push_back(*target);
                        }
                    }
                    Terminator::Branch {
                        true_target,
                        false_target,
                        ..
                    } => {
                        if !visited.contains(true_target) {
                            work_queue.push_back(*true_target);
                        }
                        if !visited.contains(false_target) {
                            work_queue.push_back(*false_target);
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(output)
    }

    /// Generate a single block with proper structure detection
    fn generate_block_with_structure(
        &mut self,
        block: &IRBlock,
        function: &IRFunction,
    ) -> Result<String, CodeGenerationError> {
        let mut output = String::new();

        // Add block label if needed (for debugging or complex control flow)
        if self.config.output.include_ir_comments && block.predecessors.len() > 1 {
            output.push_str(&self.format_indented_comment(&format!(
                "Block {} (predecessors: {:?})",
                block.id, block.predecessors
            )));
        }

        // Detect if this is a loop header
        if self.detected_loops.contains(&block.id) {
            // Try to determine loop condition from block structure
            if let Terminator::Branch {
                condition,
                true_target,
                false_target,
            } = &block.terminator
            {
                let condition_str = self.format_expression(condition)?;
                output.push_str(
                    &self.format_indented(&self.formatter.format_while_loop(&condition_str)),
                );
                self.current_indent += 1;
            }
        }

        // Generate operations
        for operation in &block.operations {
            let operation_code = self.generate_operation(operation)?;
            if !operation_code.is_empty() {
                output.push_str(
                    &self.format_indented(&self.formatter.format_statement(&operation_code)),
                );
            }
        }

        // Generate terminator with structure awareness
        let terminator_code = self.generate_structured_terminator(&block.terminator, function)?;
        if !terminator_code.is_empty() {
            output.push_str(&terminator_code);
        }

        // Close loop block if we opened one
        if self.detected_loops.contains(&block.id) {
            if matches!(block.terminator, Terminator::Branch { .. }) {
                self.current_indent -= 1;
                output.push_str(&self.format_indented(self.formatter.get_block_end()));
            }
        }

        Ok(output)
    }

    /// Generate code for single operation with comprehensive coverage
    fn generate_operation(&mut self, operation: &Operation) -> Result<String, CodeGenerationError> {
        match operation {
            Operation::Assign { target, source } => {
                let target_str = self.format_variable(target);
                let source_str = self.format_expression(source)?;
                Ok(self.formatter.format_assignment(&target_str, &source_str))
            }

            Operation::Syscall {
                name,
                arguments,
                return_type,
                target,
            } => {
                // Try to resolve syscall name from hash if it's a hex string
                let resolved_name = if name.starts_with("0x") {
                    if let Ok(hash) = u32::from_str_radix(&name[2..], 16) {
                        self.syscall_names
                            .get(&hash)
                            .map(|s| s.clone())
                            .unwrap_or_else(|| name.clone())
                    } else {
                        name.clone()
                    }
                } else {
                    name.clone()
                };

                let mut args_str = Vec::new();
                for arg in arguments {
                    args_str.push(self.format_expression(arg)?);
                }

                let call_str = self.formatter.format_function_call(
                    &resolved_name,
                    &args_str,
                    return_type.as_ref(),
                );

                if let Some(target_var) = target {
                    let target_str = self.format_variable(target_var);
                    Ok(self.formatter.format_assignment(&target_str, &call_str))
                } else {
                    Ok(call_str)
                }
            }

            Operation::ContractCall {
                contract,
                method,
                arguments,
                call_flags,
                target,
            } => {
                let contract_str = self.format_expression(contract)?;
                let args_str = arguments
                    .iter()
                    .map(|arg| self.format_expression(arg))
                    .collect::<Result<Vec<_>, _>>()?;

                // Format as structured contract call
                let call_str = format!(
                    "contract_call({}, \"{}\", [{}], CallFlags(0x{:02x}))",
                    contract_str,
                    method,
                    args_str.join(", "),
                    call_flags
                );

                if let Some(target_var) = target {
                    let target_str = self.format_variable(target_var);
                    Ok(self.formatter.format_assignment(&target_str, &call_str))
                } else {
                    Ok(call_str)
                }
            }

            Operation::Storage {
                operation: storage_op,
                key,
                value,
                target,
            } => {
                let key_str = self.format_expression(key)?;
                match storage_op {
                    StorageOp::Get => {
                        let call_str = format!("storage_get({})", key_str);
                        if let Some(target_var) = target {
                            let target_str = self.format_variable(target_var);
                            Ok(self.formatter.format_assignment(&target_str, &call_str))
                        } else {
                            Ok(call_str)
                        }
                    }
                    StorageOp::Put => {
                        if let Some(value_expr) = value {
                            let value_str = self.format_expression(value_expr)?;
                            Ok(format!("storage_put({}, {})", key_str, value_str))
                        } else {
                            Err(CodeGenerationError::InvalidOperation {
                                operation: "Storage Put requires value".to_string(),
                            })
                        }
                    }
                    StorageOp::Delete => Ok(format!("storage_delete({})", key_str)),
                    StorageOp::Find => {
                        let call_str = format!("storage_find({})", key_str);
                        if let Some(target_var) = target {
                            let target_str = self.format_variable(target_var);
                            Ok(self.formatter.format_assignment(&target_str, &call_str))
                        } else {
                            Ok(call_str)
                        }
                    }
                }
            }

            Operation::Stack {
                operation: stack_op,
                operands,
                target,
            } => match stack_op {
                StackOp::Push => {
                    if let Some(operand) = operands.first() {
                        let operand_str = self.format_expression(operand)?;
                        Ok(format!("stack_push({})", operand_str))
                    } else {
                        Ok("stack_push()".to_string())
                    }
                }
                StackOp::Pop => {
                    if let Some(target_var) = target {
                        let target_str = self.format_variable(target_var);
                        Ok(self.formatter.format_assignment(&target_str, "stack_pop()"))
                    } else {
                        Ok("stack_pop()".to_string())
                    }
                }
                StackOp::Dup => Ok("stack_dup()".to_string()),
                StackOp::Swap => Ok("stack_swap()".to_string()),
                StackOp::Drop => Ok("stack_drop()".to_string()),
                _ => Ok(format!("stack_op({:?})", stack_op)),
            },

            Operation::Arithmetic {
                operator,
                left,
                right,
                target,
            } => {
                let left_str = self.format_expression(left)?;
                let right_str = self.format_expression(right)?;
                let target_str = self.format_variable(target);
                let op_str = self.format_binary_operator(*operator);
                let expr = format!("({} {} {})", left_str, op_str, right_str);
                Ok(self.formatter.format_assignment(&target_str, &expr))
            }

            Operation::Convert {
                source,
                target_type,
                target,
            } => {
                let source_str = self.format_expression(source)?;
                let type_str = self.format_type(target_type);
                let target_str = self.format_variable(target);
                let expr = format!("convert<{}>({})", type_str, source_str);
                Ok(self.formatter.format_assignment(&target_str, &expr))
            }

            Operation::Unary {
                operator,
                operand,
                target,
            } => {
                let operand_str = self.format_expression(operand)?;
                let target_str = self.format_variable(target);
                let op_str = self.format_unary_operator(*operator);
                let expr = format!("({}{})", op_str, operand_str);
                Ok(self.formatter.format_assignment(&target_str, &expr))
            }

            Operation::BuiltinCall {
                name,
                arguments,
                target,
            } => {
                let args_str = arguments
                    .iter()
                    .map(|arg| self.format_expression(arg))
                    .collect::<Result<Vec<_>, _>>()?;

                let call_str = self.formatter.format_function_call(name, &args_str, None);

                if let Some(target_var) = target {
                    let target_str = self.format_variable(target_var);
                    Ok(self.formatter.format_assignment(&target_str, &call_str))
                } else {
                    Ok(call_str)
                }
            }

            Operation::ArrayOp {
                operation: array_op,
                array,
                index,
                value,
                target,
            } => {
                let array_str = self.format_expression(array)?;
                match array_op {
                    ArrayOperation::GetItem => {
                        if let Some(index_expr) = index {
                            let index_str = self.format_expression(index_expr)?;
                            let expr = format!("{}[{}]", array_str, index_str);
                            if let Some(target_var) = target {
                                let target_str = self.format_variable(target_var);
                                Ok(self.formatter.format_assignment(&target_str, &expr))
                            } else {
                                Ok(expr)
                            }
                        } else {
                            Err(CodeGenerationError::InvalidOperation {
                                operation: "Array GetItem requires index".to_string(),
                            })
                        }
                    }
                    ArrayOperation::SetItem => {
                        if let (Some(index_expr), Some(value_expr)) = (index, value) {
                            let index_str = self.format_expression(index_expr)?;
                            let value_str = self.format_expression(value_expr)?;
                            Ok(format!("{}[{}] = {}", array_str, index_str, value_str))
                        } else {
                            Err(CodeGenerationError::InvalidOperation {
                                operation: "Array SetItem requires index and value".to_string(),
                            })
                        }
                    }
                    ArrayOperation::Append => {
                        if let Some(value_expr) = value {
                            let value_str = self.format_expression(value_expr)?;
                            Ok(format!("{}.append({})", array_str, value_str))
                        } else {
                            Err(CodeGenerationError::InvalidOperation {
                                operation: "Array Append requires value".to_string(),
                            })
                        }
                    }
                    ArrayOperation::Size => {
                        let expr = format!("{}.length", array_str);
                        if let Some(target_var) = target {
                            let target_str = self.format_variable(target_var);
                            Ok(self.formatter.format_assignment(&target_str, &expr))
                        } else {
                            Ok(expr)
                        }
                    }
                    _ => Ok(format!("{}.{:?}()", array_str, array_op)),
                }
            }

            Operation::MapOp {
                operation: map_op,
                map,
                key,
                value,
                target,
            } => {
                let map_str = self.format_expression(map)?;
                let key_str = self.format_expression(key)?;
                match map_op {
                    MapOperation::Get => {
                        let expr = format!("{}[{}]", map_str, key_str);
                        if let Some(target_var) = target {
                            let target_str = self.format_variable(target_var);
                            Ok(self.formatter.format_assignment(&target_str, &expr))
                        } else {
                            Ok(expr)
                        }
                    }
                    MapOperation::Set => {
                        if let Some(value_expr) = value {
                            let value_str = self.format_expression(value_expr)?;
                            Ok(format!("{}[{}] = {}", map_str, key_str, value_str))
                        } else {
                            Err(CodeGenerationError::InvalidOperation {
                                operation: "Map Set requires value".to_string(),
                            })
                        }
                    }
                    MapOperation::HasKey => {
                        let expr = format!("{}.has_key({})", map_str, key_str);
                        if let Some(target_var) = target {
                            let target_str = self.format_variable(target_var);
                            Ok(self.formatter.format_assignment(&target_str, &expr))
                        } else {
                            Ok(expr)
                        }
                    }
                    _ => Ok(format!("{}.{:?}({})", map_str, map_op, key_str)),
                }
            }

            Operation::StringOp {
                operation: string_op,
                operands,
                target,
            } => {
                let target_str = self.format_variable(target);
                match string_op {
                    StringOperation::Concat => {
                        if operands.len() >= 2 {
                            let operand_strs = operands
                                .iter()
                                .map(|op| self.format_expression(op))
                                .collect::<Result<Vec<_>, _>>()?;
                            let expr = operand_strs.join(" + ");
                            Ok(self.formatter.format_assignment(&target_str, &expr))
                        } else {
                            Err(CodeGenerationError::InvalidOperation {
                                operation: "String concat requires at least 2 operands".to_string(),
                            })
                        }
                    }
                    StringOperation::Substring => {
                        if operands.len() >= 3 {
                            let string_str = self.format_expression(&operands[0])?;
                            let start_str = self.format_expression(&operands[1])?;
                            let length_str = self.format_expression(&operands[2])?;
                            let expr =
                                format!("{}.substring({}, {})", string_str, start_str, length_str);
                            Ok(self.formatter.format_assignment(&target_str, &expr))
                        } else {
                            Err(CodeGenerationError::InvalidOperation {
                                operation: "String substring requires string, start, length"
                                    .to_string(),
                            })
                        }
                    }
                    StringOperation::Length => {
                        if let Some(string_operand) = operands.first() {
                            let string_str = self.format_expression(string_operand)?;
                            let expr = format!("{}.length", string_str);
                            Ok(self.formatter.format_assignment(&target_str, &expr))
                        } else {
                            Err(CodeGenerationError::InvalidOperation {
                                operation: "String length requires string operand".to_string(),
                            })
                        }
                    }
                    _ => Ok(format!("{:?}_operation", string_op)),
                }
            }

            Operation::TypeCheck {
                value,
                target_type,
                target,
            } => {
                let value_str = self.format_expression(value)?;
                let type_str = self.format_type(target_type);
                let target_str = self.format_variable(target);
                let expr = format!("is_type<{}>({})", type_str, value_str);
                Ok(self.formatter.format_assignment(&target_str, &expr))
            }

            Operation::Throw { exception } => {
                let exception_str = self.format_expression(exception)?;
                Ok(format!("throw {}", exception_str))
            }

            Operation::Assert { condition, message } => {
                let condition_str = self.format_expression(condition)?;
                if let Some(msg_expr) = message {
                    let msg_str = self.format_expression(msg_expr)?;
                    Ok(format!("assert({}, {})", condition_str, msg_str))
                } else {
                    Ok(format!("assert({})", condition_str))
                }
            }

            Operation::Abort { message } => {
                if let Some(msg_expr) = message {
                    let msg_str = self.format_expression(msg_expr)?;
                    Ok(format!("abort({})", msg_str))
                } else {
                    Ok("abort()".to_string())
                }
            }

            Operation::Effect {
                effect_type,
                description,
            } => Ok(self.formatter.format_security_annotation(&format!(
                "@effect {:?}: {}",
                effect_type, description
            ))),

            Operation::Comment(comment) => Ok(self.formatter.format_comment(comment)),
        }
    }

    /// Generate structured terminator with control flow awareness
    fn generate_structured_terminator(
        &mut self,
        terminator: &Terminator,
        function: &IRFunction,
    ) -> Result<String, CodeGenerationError> {
        match terminator {
            Terminator::Return(expr) => {
                let return_str = if let Some(return_expr) = expr {
                    let formatted_expr = self.format_expression(return_expr)?;
                    self.formatter.format_return(Some(&formatted_expr))
                } else {
                    self.formatter.format_return(None)
                };
                Ok(self.format_indented(&return_str))
            }

            Terminator::Jump(target) => {
                // In structured output, jumps are usually handled by control structures
                // Only emit goto for complex cases
                if self.config.output.include_ir_comments {
                    Ok(self.format_indented_comment(&format!("goto block_{}", target)))
                } else {
                    Ok(String::new())
                }
            }

            Terminator::Branch {
                condition,
                true_target,
                false_target,
            } => {
                let condition_str = self.format_expression(condition)?;

                // Check if this is part of a loop structure
                if self.detected_loops.contains(&true_target)
                    || self.detected_loops.contains(&false_target)
                {
                    // This is likely handled by loop structure
                    Ok(String::new())
                } else {
                    // Generate if-else structure
                    let if_statement = self.formatter.format_if_statement(&condition_str, self);
                    let mut result = self.format_indented(&if_statement);
                    result.push_str(&self.format_indented(self.formatter.get_block_start()));
                    result.push('\n');

                    self.current_indent += 1;
                    result.push_str(
                        &self.format_indented_comment(&format!(
                            "true branch -> block_{}",
                            true_target
                        )),
                    );
                    self.current_indent -= 1;

                    result.push_str(&self.format_indented(self.formatter.get_block_end()));
                    result.push_str(" else ");
                    result.push_str(self.formatter.get_block_start());
                    result.push('\n');

                    self.current_indent += 1;
                    result.push_str(&self.format_indented_comment(&format!(
                        "false branch -> block_{}",
                        false_target
                    )));
                    self.current_indent -= 1;

                    result.push_str(&self.format_indented(self.formatter.get_block_end()));
                    result.push('\n');

                    Ok(result)
                }
            }

            Terminator::Abort(expr) => {
                let abort_str = if let Some(abort_expr) = expr {
                    format!("abort({})", self.format_expression(abort_expr)?)
                } else {
                    "abort()".to_string()
                };
                Ok(self.format_indented(&self.formatter.format_statement(&abort_str)))
            }

            Terminator::Switch {
                discriminant,
                targets,
                default_target,
            } => {
                let disc_str = self.format_expression(discriminant)?;
                let mut result = self.format_indented(&format!(
                    "switch ({}) {}",
                    disc_str,
                    self.formatter.get_block_start()
                ));
                result.push('\n');

                self.current_indent += 1;

                for (literal, target) in targets {
                    let case_str = self.format_literal(literal);
                    result.push_str(
                        &self
                            .format_indented(&format!("case {}: goto block_{};", case_str, target)),
                    );
                }

                if let Some(default) = default_target {
                    result.push_str(
                        &self.format_indented(&format!("default: goto block_{};", default)),
                    );
                }

                self.current_indent -= 1;
                result.push_str(&self.format_indented(self.formatter.get_block_end()));
                result.push('\n');

                Ok(result)
            }

            Terminator::TryBlock {
                try_block,
                catch_block,
                finally_block,
            } => {
                let mut result =
                    self.format_indented(&format!("try {}", self.formatter.get_block_start()));
                result.push('\n');

                self.current_indent += 1;
                result.push_str(
                    &self.format_indented_comment(&format!("try block -> block_{}", try_block)),
                );
                self.current_indent -= 1;

                result.push_str(&self.format_indented(self.formatter.get_block_end()));

                if let Some(catch) = catch_block {
                    result.push_str(" catch ");
                    result.push_str(self.formatter.get_block_start());
                    result.push('\n');

                    self.current_indent += 1;
                    result.push_str(
                        &self.format_indented_comment(&format!("catch block -> block_{}", catch)),
                    );
                    self.current_indent -= 1;

                    result.push_str(&self.format_indented(self.formatter.get_block_end()));
                }

                if let Some(finally) = finally_block {
                    result.push_str(" finally ");
                    result.push_str(self.formatter.get_block_start());
                    result.push('\n');

                    self.current_indent += 1;
                    result.push_str(
                        &self.format_indented_comment(&format!(
                            "finally block -> block_{}",
                            finally
                        )),
                    );
                    self.current_indent -= 1;

                    result.push_str(&self.format_indented(self.formatter.get_block_end()));
                }

                result.push('\n');
                Ok(result)
            }
        }
    }

    /// Format expression with comprehensive coverage and type awareness
    fn format_expression(&mut self, expr: &Expression) -> Result<String, CodeGenerationError> {
        match expr {
            Expression::Literal(literal) => Ok(self.format_literal(literal)),

            Expression::Variable(var) => Ok(self.format_variable(var)),

            Expression::BinaryOp { left, op, right } => {
                let left_str = self.format_expression(left)?;
                let right_str = self.format_expression(right)?;
                let op_str = self.format_binary_operator(*op);

                // Add parentheses based on operator precedence
                Ok(format!("({} {} {})", left_str, op_str, right_str))
            }

            Expression::UnaryOp { op, operand } => {
                let operand_str = self.format_expression(operand)?;
                let op_str = self.format_unary_operator(*op);
                Ok(format!("{}{}", op_str, operand_str))
            }

            Expression::Call {
                function,
                arguments,
            } => {
                let args_str = arguments
                    .iter()
                    .map(|arg| self.format_expression(arg))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(self
                    .formatter
                    .format_function_call(function, &args_str, None))
            }

            Expression::Index { array, index } => {
                let array_str = self.format_expression(array)?;
                let index_str = self.format_expression(index)?;
                Ok(format!("{}[{}]", array_str, index_str))
            }

            Expression::Field { object, field } => {
                let object_str = self.format_expression(object)?;
                Ok(format!("{}.{}", object_str, field))
            }

            Expression::Cast {
                target_type,
                expression,
            } => {
                let expr_str = self.format_expression(expression)?;
                let type_str = self.format_type(target_type);
                Ok(format!("({})cast<{}>({})", type_str, type_str, expr_str))
            }

            Expression::Array(elements) => {
                let elements_str = elements
                    .iter()
                    .map(|elem| self.format_expression(elem))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!("[{}]", elements_str.join(", ")))
            }

            Expression::Map(entries) => {
                let entries_str = entries
                    .iter()
                    .map(|(key, value)| {
                        Ok(format!(
                            "{}: {}",
                            self.format_expression(key)?,
                            self.format_expression(value)?
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!("{{{}}}", entries_str.join(", ")))
            }

            Expression::Struct { fields } => {
                let fields_str = fields
                    .iter()
                    .map(|(name, expr)| Ok(format!("{}: {}", name, self.format_expression(expr)?)))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!("{{ {} }}", fields_str.join(", ")))
            }

            Expression::ArrayCreate { element_type, size } => {
                let type_str = self.format_type(element_type);
                let size_str = self.format_expression(size)?;
                Ok(format!("new Array<{}>([{}])", type_str, size_str))
            }

            Expression::StructCreate { fields } => {
                let fields_str = fields
                    .iter()
                    .map(|(name, expr)| Ok(format!("{}: {}", name, self.format_expression(expr)?)))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!("{{ {} }}", fields_str.join(", ")))
            }

            Expression::MapCreate { entries } => {
                let entries_str = entries
                    .iter()
                    .map(|(key, value)| {
                        Ok(format!(
                            "{}: {}",
                            self.format_expression(key)?,
                            self.format_expression(value)?
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!("new Map {{ {} }}", entries_str.join(", ")))
            }
        }
    }

    /// Helper functions for formatting and indentation
    fn format_indented(&self, text: &str) -> String {
        let indent = " ".repeat(self.current_indent * self.config.output.indent_size);
        format!("{}{}\n", indent, text)
    }

    fn format_indented_comment(&self, text: &str) -> String {
        self.format_indented(&self.formatter.format_comment(text))
    }

    fn format_comment(&self, text: &str) -> String {
        self.formatter.format_comment(text)
    }

    /// Format type name with comprehensive type system support
    fn format_type(&self, type_ref: &Type) -> String {
        match type_ref {
            Type::Primitive(prim) => match prim {
                PrimitiveType::Boolean => "bool".to_string(),
                PrimitiveType::Integer => "integer".to_string(),
                PrimitiveType::ByteString => "string".to_string(),
                PrimitiveType::Hash160 => "Hash160".to_string(),
                PrimitiveType::Hash256 => "Hash256".to_string(),
                PrimitiveType::ECPoint => "ECPoint".to_string(),
                PrimitiveType::PublicKey => "PublicKey".to_string(),
                PrimitiveType::Signature => "Signature".to_string(),
                PrimitiveType::Null => "null".to_string(),
                PrimitiveType::String => "string".to_string(),
                PrimitiveType::ByteArray => "ByteArray".to_string(),
            },
            Type::Array(element_type) => format!("Array<{}>", self.format_type(element_type)),
            Type::Map { key, value } => format!(
                "Map<{}, {}>",
                self.format_type(key),
                self.format_type(value)
            ),
            Type::Buffer => "Buffer".to_string(),
            Type::Struct(struct_type) => struct_type
                .name
                .clone()
                .unwrap_or_else(|| "Struct".to_string()),
            Type::Union(types) => {
                let type_strs: Vec<String> = types.iter().map(|t| self.format_type(t)).collect();
                format!("({})", type_strs.join(" | "))
            }
            Type::Function {
                parameters,
                return_type,
            } => {
                let param_strs: Vec<String> =
                    parameters.iter().map(|p| self.format_type(p)).collect();
                format!(
                    "({}) => {}",
                    param_strs.join(", "),
                    self.format_type(return_type)
                )
            }
            Type::Contract(contract) => contract.name.clone(),
            Type::InteropInterface(name) => format!("InteropInterface<{}>", name),
            Type::Pointer(inner) => format!("*{}", self.format_type(inner)),
            Type::Nullable(inner) => format!("{}?", self.format_type(inner)),
            Type::Generic { base, parameters } => {
                let param_strs: Vec<String> =
                    parameters.iter().map(|p| self.format_type(p)).collect();
                format!("{}<{}>", base, param_strs.join(", "))
            }
            Type::UserDefined(name) => name.clone(),
            Type::Unknown => "unknown".to_string(),
            Type::Variable(var) => format!("T{}", var),
            Type::Any => "any".to_string(),
            Type::Never => "never".to_string(),
            Type::Void => "void".to_string(),
        }
    }

    /// Format variable reference with intelligent naming
    fn format_variable(&mut self, var: &Variable) -> String {
        // Check if we have a custom name mapping
        if let Some(custom_name) = self.variable_names.get(&var.name) {
            return custom_name.clone();
        }

        // Generate production-ready names for variables
        let formatted_name = if var.name.starts_with("result_") {
            format!("result{}", var.id)
        } else if var.name.starts_with("local_") {
            format!("local{}", var.id)
        } else if var.name.starts_with("arg_") {
            format!("arg{}", var.id)
        } else if var.name.starts_with("static_") {
            format!("static{}", var.id)
        } else {
            var.name.clone()
        };

        // Store mapping for consistency
        if formatted_name != var.name {
            self.variable_names
                .insert(var.name.clone(), formatted_name.clone());
            self.var_counter += 1;
        }

        // Add type annotation if enabled
        if self.config.output.include_type_annotations {
            // Type information would come from type inference pass
            // In a more complete implementation, this would show the inferred type
            formatted_name
        } else {
            formatted_name
        }
    }

    /// Format literal value
    fn format_literal(&self, literal: &Literal) -> String {
        match literal {
            Literal::Boolean(b) => b.to_string(),
            Literal::Integer(i) => i.to_string(),
            Literal::BigInteger(bytes) => format!("0x{}", hex::encode(bytes)),
            Literal::String(s) => format!("\"{}\"", s),
            Literal::ByteArray(bytes) => format!("0x{}", hex::encode(bytes)),
            Literal::Hash160(hash) => format!("0x{}", hex::encode(hash)),
            Literal::Hash256(hash) => format!("0x{}", hex::encode(hash)),
            Literal::Null => "null".to_string(),
        }
    }

    /// Format binary operator
    fn format_binary_operator(&self, op: BinaryOperator) -> &'static str {
        match op {
            BinaryOperator::Add => "+",
            BinaryOperator::Sub => "-",
            BinaryOperator::Mul => "*",
            BinaryOperator::Div => "/",
            // Alternative names for compatibility
            BinaryOperator::Subtract => "-",
            BinaryOperator::Multiply => "*",
            BinaryOperator::Divide => "/",
            BinaryOperator::Mod => "%",
            BinaryOperator::Pow => "**",
            BinaryOperator::Equal => "==",
            BinaryOperator::NotEqual => "!=",
            BinaryOperator::Less => "<",
            BinaryOperator::LessEqual => "<=",
            BinaryOperator::Greater => ">",
            BinaryOperator::GreaterEqual => ">=",
            BinaryOperator::And => "&",
            BinaryOperator::Or => "|",
            BinaryOperator::Xor => "^",
            BinaryOperator::BoolAnd => "&&",
            BinaryOperator::BoolOr => "||",
            BinaryOperator::LeftShift => "<<",
            BinaryOperator::RightShift => ">>",
        }
    }

    /// Format unary operator
    fn format_unary_operator(&self, op: UnaryOperator) -> &'static str {
        match op {
            UnaryOperator::Not => "~",
            UnaryOperator::Negate => "-",
            UnaryOperator::BitwiseNot => "~",
            UnaryOperator::BoolNot => "!",
            UnaryOperator::Abs => "abs",
            UnaryOperator::Sign => "sign",
            UnaryOperator::Sqrt => "sqrt",
        }
    }
}

/// C-style syntax formatter
struct CStyleFormatter;

impl SyntaxFormatter for CStyleFormatter {
    fn format_function_signature(
        &self,
        function: &IRFunction,
        generator: &PseudocodeGenerator,
    ) -> String {
        let mut signature = String::new();

        // Return type
        if let Some(return_type) = &function.return_type {
            signature.push_str(&generator.format_type(return_type));
        } else {
            signature.push_str("void");
        }

        signature.push(' ');
        signature.push_str(&function.name);
        signature.push('(');

        // Parameters
        for (i, param) in function.parameters.iter().enumerate() {
            if i > 0 {
                signature.push_str(", ");
            }
            signature.push_str(&generator.format_type(&param.param_type));
            signature.push(' ');
            signature.push_str(&param.name);
        }

        signature.push(')');
        signature
    }

    fn format_block_start(&self, _block_id: BlockId) -> String {
        "{".to_string()
    }
    fn format_block_end(&self) -> String {
        "}".to_string()
    }
    fn format_statement(&self, statement: &str) -> String {
        format!("{}{}", statement, self.get_line_ending())
    }
    fn format_assignment(&self, target: &str, source: &str) -> String {
        format!("{} = {}", target, source)
    }
    fn format_function_call(
        &self,
        name: &str,
        args: &[String],
        _return_type: Option<&Type>,
    ) -> String {
        format!("{}({})", name, args.join(", "))
    }
    fn format_if_statement(&self, condition: &str, _generator: &PseudocodeGenerator) -> String {
        format!("if ({})", condition)
    }
    fn format_while_loop(&self, condition: &str) -> String {
        format!("while ({})", condition)
    }
    fn format_return(&self, expr: Option<&str>) -> String {
        if let Some(e) = expr {
            format!("return {}", e)
        } else {
            "return".to_string()
        }
    }
    fn format_type_annotation(&self, var_name: &str, type_name: &str) -> String {
        format!("{} {}", type_name, var_name)
    }
    fn format_comment(&self, text: &str) -> String {
        format!("// {}", text)
    }
    fn format_security_annotation(&self, annotation: &str) -> String {
        format!("/* {} */", annotation)
    }
    fn get_line_ending(&self) -> &'static str {
        ";"
    }
    fn get_block_start(&self) -> &'static str {
        " {"
    }
    fn get_block_end(&self) -> &'static str {
        "}"
    }
}

/// Python-style syntax formatter
struct PythonFormatter;

impl SyntaxFormatter for PythonFormatter {
    fn format_function_signature(
        &self,
        function: &IRFunction,
        generator: &PseudocodeGenerator,
    ) -> String {
        let mut signature = String::new();
        signature.push_str("def ");
        signature.push_str(&function.name);
        signature.push('(');

        // Parameters with type hints
        for (i, param) in function.parameters.iter().enumerate() {
            if i > 0 {
                signature.push_str(", ");
            }
            signature.push_str(&param.name);
            signature.push_str(": ");
            signature.push_str(&generator.format_type(&param.param_type));
        }

        signature.push(')');

        // Return type annotation
        if let Some(return_type) = &function.return_type {
            signature.push_str(" -> ");
            signature.push_str(&generator.format_type(return_type));
        }

        signature
    }

    fn format_block_start(&self, _block_id: BlockId) -> String {
        ":".to_string()
    }
    fn format_block_end(&self) -> String {
        "".to_string()
    }
    fn format_statement(&self, statement: &str) -> String {
        statement.to_string()
    }
    fn format_assignment(&self, target: &str, source: &str) -> String {
        format!("{} = {}", target, source)
    }
    fn format_function_call(
        &self,
        name: &str,
        args: &[String],
        _return_type: Option<&Type>,
    ) -> String {
        format!("{}({})", name, args.join(", "))
    }
    fn format_if_statement(&self, condition: &str, _generator: &PseudocodeGenerator) -> String {
        format!("if {}:", condition)
    }
    fn format_while_loop(&self, condition: &str) -> String {
        format!("while {}:", condition)
    }
    fn format_return(&self, expr: Option<&str>) -> String {
        if let Some(e) = expr {
            format!("return {}", e)
        } else {
            "return".to_string()
        }
    }
    fn format_type_annotation(&self, var_name: &str, type_name: &str) -> String {
        format!("{}: {}", var_name, type_name)
    }
    fn format_comment(&self, text: &str) -> String {
        format!("# {}", text)
    }
    fn format_security_annotation(&self, annotation: &str) -> String {
        format!("# {}", annotation)
    }
    fn get_line_ending(&self) -> &'static str {
        ""
    }
    fn get_block_start(&self) -> &'static str {
        ":"
    }
    fn get_block_end(&self) -> &'static str {
        ""
    }
}

/// Rust-style syntax formatter
struct RustFormatter;

impl SyntaxFormatter for RustFormatter {
    fn format_function_signature(
        &self,
        function: &IRFunction,
        generator: &PseudocodeGenerator,
    ) -> String {
        let mut signature = String::new();
        signature.push_str("fn ");
        signature.push_str(&function.name);
        signature.push('(');

        // Parameters
        for (i, param) in function.parameters.iter().enumerate() {
            if i > 0 {
                signature.push_str(", ");
            }
            signature.push_str(&param.name);
            signature.push_str(": ");
            signature.push_str(&generator.format_type(&param.param_type));
        }

        signature.push(')');

        // Return type
        if let Some(return_type) = &function.return_type {
            signature.push_str(" -> ");
            signature.push_str(&generator.format_type(return_type));
        }

        signature
    }

    fn format_block_start(&self, _block_id: BlockId) -> String {
        "{".to_string()
    }
    fn format_block_end(&self) -> String {
        "}".to_string()
    }
    fn format_statement(&self, statement: &str) -> String {
        format!("{}{}", statement, self.get_line_ending())
    }
    fn format_assignment(&self, target: &str, source: &str) -> String {
        format!("let {} = {}", target, source)
    }
    fn format_function_call(
        &self,
        name: &str,
        args: &[String],
        _return_type: Option<&Type>,
    ) -> String {
        format!("{}({})", name, args.join(", "))
    }
    fn format_if_statement(&self, condition: &str, _generator: &PseudocodeGenerator) -> String {
        format!("if {} {{", condition)
    }
    fn format_while_loop(&self, condition: &str) -> String {
        format!("while {} {{", condition)
    }
    fn format_return(&self, expr: Option<&str>) -> String {
        if let Some(e) = expr {
            format!("return {}", e)
        } else {
            "return".to_string()
        }
    }
    fn format_type_annotation(&self, var_name: &str, type_name: &str) -> String {
        format!("{}: {}", var_name, type_name)
    }
    fn format_comment(&self, text: &str) -> String {
        format!("// {}", text)
    }
    fn format_security_annotation(&self, annotation: &str) -> String {
        format!("// {}", annotation)
    }
    fn get_line_ending(&self) -> &'static str {
        ";"
    }
    fn get_block_start(&self) -> &'static str {
        " {"
    }
    fn get_block_end(&self) -> &'static str {
        "}"
    }
}

/// TypeScript-style syntax formatter
struct TypeScriptFormatter;

impl SyntaxFormatter for TypeScriptFormatter {
    fn format_function_signature(
        &self,
        function: &IRFunction,
        generator: &PseudocodeGenerator,
    ) -> String {
        let mut signature = String::new();
        signature.push_str("function ");
        signature.push_str(&function.name);
        signature.push('(');

        // Parameters with types
        for (i, param) in function.parameters.iter().enumerate() {
            if i > 0 {
                signature.push_str(", ");
            }
            signature.push_str(&param.name);
            signature.push_str(": ");
            signature.push_str(&generator.format_type(&param.param_type));
        }

        signature.push(')');

        // Return type
        if let Some(return_type) = &function.return_type {
            signature.push_str(": ");
            signature.push_str(&generator.format_type(return_type));
        }

        signature
    }

    fn format_block_start(&self, _block_id: BlockId) -> String {
        "{".to_string()
    }
    fn format_block_end(&self) -> String {
        "}".to_string()
    }
    fn format_statement(&self, statement: &str) -> String {
        format!("{}{}", statement, self.get_line_ending())
    }
    fn format_assignment(&self, target: &str, source: &str) -> String {
        format!("{} = {}", target, source)
    }
    fn format_function_call(
        &self,
        name: &str,
        args: &[String],
        _return_type: Option<&Type>,
    ) -> String {
        format!("{}({})", name, args.join(", "))
    }
    fn format_if_statement(&self, condition: &str, _generator: &PseudocodeGenerator) -> String {
        format!("if ({})", condition)
    }
    fn format_while_loop(&self, condition: &str) -> String {
        format!("while ({})", condition)
    }
    fn format_return(&self, expr: Option<&str>) -> String {
        if let Some(e) = expr {
            format!("return {}", e)
        } else {
            "return".to_string()
        }
    }
    fn format_type_annotation(&self, var_name: &str, type_name: &str) -> String {
        format!("{}: {}", var_name, type_name)
    }
    fn format_comment(&self, text: &str) -> String {
        format!("// {}", text)
    }
    fn format_security_annotation(&self, annotation: &str) -> String {
        format!("/* {} */", annotation)
    }
    fn get_line_ending(&self) -> &'static str {
        ";"
    }
    fn get_block_start(&self) -> &'static str {
        " {"
    }
    fn get_block_end(&self) -> &'static str {
        "}"
    }
}

// Utility functions
mod hex {
    pub fn encode(data: &[u8]) -> String {
        data.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::types::{PrimitiveType, Type};
    use crate::common::config::SyntaxStyle;
    use crate::common::types::*;

    #[test]
    fn test_pseudocode_generator_creation() {
        let config = DecompilerConfig::default();
        let mut generator = PseudocodeGenerator::new(&config);
        // Should create successfully
        assert!(!generator.syscall_names.is_empty());
    }

    #[test]
    fn test_format_literal() {
        let config = DecompilerConfig::default();
        let mut generator = PseudocodeGenerator::new(&config);

        assert_eq!(generator.format_literal(&Literal::Boolean(true)), "true");
        assert_eq!(generator.format_literal(&Literal::Integer(42)), "42");
        assert_eq!(
            generator.format_literal(&Literal::String("test".to_string())),
            "\"test\""
        );
        assert_eq!(
            generator.format_literal(&Literal::ByteArray(vec![0x12, 0x34])),
            "0x1234"
        );
        assert_eq!(generator.format_literal(&Literal::Null), "null");
    }

    #[test]
    fn test_format_binary_operator() {
        let config = DecompilerConfig::default();
        let mut generator = PseudocodeGenerator::new(&config);

        assert_eq!(generator.format_binary_operator(BinaryOperator::Add), "+");
        assert_eq!(
            generator.format_binary_operator(BinaryOperator::Equal),
            "=="
        );
        assert_eq!(
            generator.format_binary_operator(BinaryOperator::BoolAnd),
            "&&"
        );
    }

    #[test]
    fn test_format_type() {
        let config = DecompilerConfig::default();
        let mut generator = PseudocodeGenerator::new(&config);

        assert_eq!(
            generator.format_type(&Type::Primitive(PrimitiveType::Boolean)),
            "bool"
        );
        assert_eq!(
            generator.format_type(&Type::Primitive(PrimitiveType::Integer)),
            "integer"
        );
        assert_eq!(
            generator.format_type(&Type::Array(Box::new(Type::Primitive(
                PrimitiveType::Integer
            )))),
            "Array<integer>"
        );
    }

    #[test]
    fn test_syscall_name_resolution() {
        let config = DecompilerConfig::default();
        let mut generator = PseudocodeGenerator::new(&config);

        // Test known syscall hash resolution
        assert!(generator.syscall_names.contains_key(&0x925de831)); // System.Storage.Get
        assert_eq!(generator.syscall_names[&0x925de831], "System.Storage.Get");
    }

    #[test]
    fn test_simple_function_generation() {
        let config = DecompilerConfig::default();
        let mut generator = PseudocodeGenerator::new(&config);

        let function = IRFunction::new("test_function".to_string());
        let result = generator.generate(&function);

        assert!(result.is_ok());
        let pseudocode = result.unwrap();
        assert!(pseudocode.contains("test_function"));
        assert!(pseudocode.contains("void")); // Default return type
    }

    #[test]
    fn test_different_syntax_styles() {
        // Test C-style
        let mut c_config = DecompilerConfig::default();
        c_config.output.syntax_style = SyntaxStyle::CStyle;
        let mut c_generator = PseudocodeGenerator::new(&c_config);
        let c_function = IRFunction::new("test".to_string());
        let c_result = c_generator.generate(&c_function).unwrap();
        assert!(c_result.contains("void test()"));

        // Test Python-style
        let mut py_config = DecompilerConfig::default();
        py_config.output.syntax_style = SyntaxStyle::Python;
        let mut py_generator = PseudocodeGenerator::new(&py_config);
        let py_function = IRFunction::new("test".to_string());
        let py_result = py_generator.generate(&py_function).unwrap();
        assert!(py_result.contains("def test():"));

        // Test Rust-style
        let mut rust_config = DecompilerConfig::default();
        rust_config.output.syntax_style = SyntaxStyle::Rust;
        let mut rust_generator = PseudocodeGenerator::new(&rust_config);
        let rust_function = IRFunction::new("test".to_string());
        let rust_result = rust_generator.generate(&rust_function).unwrap();
        assert!(rust_result.contains("fn test()"));

        // Test TypeScript-style
        let mut ts_config = DecompilerConfig::default();
        ts_config.output.syntax_style = SyntaxStyle::TypeScript;
        let mut ts_generator = PseudocodeGenerator::new(&ts_config);
        let ts_function = IRFunction::new("test".to_string());
        let ts_result = ts_generator.generate(&ts_function).unwrap();
        assert!(ts_result.contains("function test()"));
    }

    #[test]
    fn test_complex_operations() {
        let config = DecompilerConfig::default();
        let mut generator = PseudocodeGenerator::new(&config);

        // Test syscall operation
        let syscall_op = Operation::Syscall {
            name: "0x925de831".to_string(), // Storage.Get hash
            arguments: vec![Expression::Literal(Literal::String("key".to_string()))],
            return_type: Some(Type::Primitive(PrimitiveType::ByteString)),
            target: Some(Variable {
                name: "result".to_string(),
                id: 1,
                var_type: VariableType::Local,
            }),
        };

        let result = generator.generate_operation(&syscall_op).unwrap();
        assert!(result.contains("System.Storage.Get"));
        assert!(result.contains("result"));

        // Test contract call operation
        let contract_call = Operation::ContractCall {
            contract: Expression::Literal(Literal::ByteArray(vec![0x12; 20])),
            method: "transfer".to_string(),
            arguments: vec![
                Expression::Variable(Variable {
                    name: "from".to_string(),
                    id: 1,
                    var_type: VariableType::Parameter,
                }),
                Expression::Variable(Variable {
                    name: "to".to_string(),
                    id: 2,
                    var_type: VariableType::Parameter,
                }),
                Expression::Literal(Literal::Integer(100)),
            ],
            call_flags: 0x01,
            target: Some(Variable {
                name: "success".to_string(),
                id: 3,
                var_type: VariableType::Local,
            }),
        };

        let contract_result = generator.generate_operation(&contract_call).unwrap();
        assert!(contract_result.contains("contract_call"));
        assert!(contract_result.contains("transfer"));
        assert!(contract_result.contains("success"));
    }

    #[test]
    fn test_expression_formatting() {
        let config = DecompilerConfig::default();
        let mut generator = PseudocodeGenerator::new(&config);

        // Test binary operation expression
        let binary_expr = Expression::BinaryOp {
            left: Box::new(Expression::Variable(Variable {
                name: "a".to_string(),
                id: 1,
                var_type: VariableType::Local,
            })),
            op: BinaryOperator::Add,
            right: Box::new(Expression::Literal(Literal::Integer(10))),
        };

        let result = generator.format_expression(&binary_expr).unwrap();
        assert!(result.contains("a"));
        assert!(result.contains("+"));
        assert!(result.contains("10"));

        // Test array expression
        let array_expr = Expression::Array(vec![
            Expression::Literal(Literal::Integer(1)),
            Expression::Literal(Literal::Integer(2)),
            Expression::Literal(Literal::Integer(3)),
        ]);

        let array_result = generator.format_expression(&array_expr).unwrap();
        assert!(array_result.contains("[1, 2, 3]"));
    }

    #[test]
    fn test_terminator_formatting() {
        let config = DecompilerConfig::default();
        let mut generator = PseudocodeGenerator::new(&config);
        let function = IRFunction::new("test".to_string());

        // Test return terminator
        let return_term = Terminator::Return(Some(Expression::Literal(Literal::Integer(42))));
        let result = generator
            .generate_structured_terminator(&return_term, &function)
            .unwrap();
        assert!(result.contains("return"));
        assert!(result.contains("42"));

        // Test branch terminator
        let branch_term = Terminator::Branch {
            condition: Expression::Variable(Variable {
                name: "condition".to_string(),
                id: 1,
                var_type: VariableType::Local,
            }),
            true_target: 1,
            false_target: 2,
        };
        let branch_result = generator
            .generate_structured_terminator(&branch_term, &function)
            .unwrap();
        assert!(branch_result.contains("if"));
        assert!(branch_result.contains("condition"));
    }

    #[test]
    fn test_variable_name_mapping() {
        let config = DecompilerConfig::default();
        let mut generator = PseudocodeGenerator::new(&config);

        let result_var = Variable {
            name: "result_123".to_string(),
            id: 123,
            var_type: VariableType::Temporary,
        };

        let formatted_name = generator.format_variable(&result_var);
        assert!(formatted_name.starts_with("result"));

        // Test consistency - should return same name on subsequent calls
        let formatted_name2 = generator.format_variable(&result_var);
        assert_eq!(formatted_name, formatted_name2);
    }
}

/// Additional error types for pseudocode generation
mod errors {
    use crate::common::errors::CodeGenerationError;

    impl CodeGenerationError {
        pub fn invalid_operation(operation: String) -> Self {
            CodeGenerationError::InvalidOperation { operation }
        }

        pub fn unsupported_feature(feature: String) -> Self {
            CodeGenerationError::UnsupportedFeature { feature }
        }
    }
}
