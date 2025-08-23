//! IR lifter - converts disassembled instructions to intermediate representation

use crate::analysis::types::{PrimitiveType, StructType, Type};
use crate::common::{config::DecompilerConfig, errors::LiftError, types::*};
use crate::core::{ir::*, syscalls::SyscallDatabase};
use std::collections::HashMap;

/// IR lifter converts instructions to intermediate representation
pub struct IRLifter {
    /// Configuration
    config: DecompilerConfig,
    /// Syscall database
    syscall_db: SyscallDatabase,
    /// Current variable counter
    var_counter: u32,
    /// Stack simulation
    stack: Vec<Expression>,
    /// Alt stack for Neo N3
    alt_stack: Vec<Expression>,
    /// Maximum stack depth reached
    max_stack_depth: usize,
    /// Variable mappings (slot -> variable)
    variables: HashMap<u32, Variable>,
    /// Argument variables (slot -> variable)
    arguments: HashMap<u32, Variable>,
    /// Static field variables (slot -> variable)
    static_fields: HashMap<u32, Variable>,
    /// Offset to block ID mapping
    offset_to_block: HashMap<u32, BlockId>,
    /// Current instruction offset (for error context)
    current_offset: u32,
}

impl IRLifter {
    /// Create new IR lifter with configuration
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
            var_counter: 0,
            stack: Vec::new(),
            alt_stack: Vec::new(),
            max_stack_depth: 0,
            variables: HashMap::new(),
            arguments: HashMap::new(),
            static_fields: HashMap::new(),
            offset_to_block: HashMap::new(),
            current_offset: 0,
        }
    }

    /// Convert instruction sequence to IR function
    pub fn lift_to_ir(&mut self, instructions: &[Instruction]) -> Result<IRFunction, LiftError> {
        let mut function = IRFunction::new("main".to_string());

        // Reset state for new function
        self.reset_state();

        // Build offset to block mapping first
        let block_boundaries = self.find_block_boundaries(instructions);
        self.build_offset_to_block_mapping(instructions, &block_boundaries);

        // Build basic blocks from instructions
        let mut blocks = self.build_basic_blocks(instructions, &block_boundaries)?;

        // Build predecessor relationships
        self.build_predecessor_relationships(&mut blocks);

        // Add blocks to function
        for block in blocks {
            function.add_block(block);
        }

        // Set entry block
        if !function.blocks.is_empty() {
            function.entry_block = 0;
        }

        // Validate mapping completeness
        self.validate_offset_mapping(instructions)?;

        // Calculate complexity metrics
        function.calculate_complexity();

        Ok(function)
    }

    /// Reset lifter state for new function
    fn reset_state(&mut self) {
        self.var_counter = 0;
        self.stack.clear();
        self.alt_stack.clear();
        self.max_stack_depth = 0;
        self.variables.clear();
        self.arguments.clear();
        self.static_fields.clear();
        self.offset_to_block.clear();
        self.current_offset = 0;
    }

    /// Build complete offset to block ID mapping
    fn build_offset_mapping(&mut self, boundaries: &[u32]) {
        // Clear existing mapping
        self.offset_to_block.clear();

        // Create mapping for each boundary to its block ID
        for (block_id, &start_offset) in boundaries.iter().enumerate() {
            let end_offset = boundaries.get(block_id + 1).copied().unwrap_or(u32::MAX);

            // Map the start offset to this block
            self.offset_to_block
                .insert(start_offset, block_id as BlockId);

            // For complete mapping, we need to map ALL instruction offsets in this range
            // This ensures any offset-based lookup finds the correct block
            for offset in start_offset..end_offset {
                self.offset_to_block.insert(offset, block_id as BlockId);
            }
        }
    }

    /// Build complete offset to block mapping from instructions and boundaries
    fn build_offset_to_block_mapping(&mut self, instructions: &[Instruction], boundaries: &[u32]) {
        self.offset_to_block.clear();

        // For each block boundary, determine which instructions belong to that block
        for (block_id, &start_offset) in boundaries.iter().enumerate() {
            let end_offset = boundaries.get(block_id + 1).copied().unwrap_or(u32::MAX);

            // Map all instruction offsets in this block range
            for instruction in instructions {
                if instruction.offset >= start_offset && instruction.offset < end_offset {
                    self.offset_to_block
                        .insert(instruction.offset, block_id as BlockId);

                    // Also map intermediate offsets within this instruction
                    for offset in instruction.offset..(instruction.offset + instruction.size as u32)
                    {
                        self.offset_to_block.insert(offset, block_id as BlockId);
                    }
                }
            }
        }
    }

    /// Find basic block boundaries in instruction stream
    fn find_block_boundaries(&self, instructions: &[Instruction]) -> Vec<u32> {
        let mut boundaries = vec![0]; // Entry point
        let mut jump_targets = std::collections::HashSet::new();

        // First pass: collect all jump targets and control flow changes
        for instruction in instructions {
            match instruction.opcode {
                // All jump instructions create new blocks at target and after jump
                OpCode::JMP
                | OpCode::JMP_L
                | OpCode::JMPIF
                | OpCode::JMPIF_L
                | OpCode::JMPIFNOT
                | OpCode::JMPIFNOT_L
                | OpCode::JMPEQ
                | OpCode::JMPEQ_L
                | OpCode::JMPNE
                | OpCode::JMPNE_L
                | OpCode::JMPGT
                | OpCode::JMPGT_L
                | OpCode::JMPGE
                | OpCode::JMPGE_L
                | OpCode::JMPLT
                | OpCode::JMPLT_L
                | OpCode::JMPLE
                | OpCode::JMPLE_L => {
                    // Add jump target as boundary
                    if let Some(target_offset) = self.extract_jump_target(instruction) {
                        jump_targets.insert(target_offset);
                        boundaries.push(target_offset);
                    }

                    // Add instruction after jump as boundary (fallthrough for conditional jumps)
                    match instruction.opcode {
                        OpCode::JMP | OpCode::JMP_L => {
                            // Unconditional jump - no fallthrough
                        }
                        _ => {
                            // Conditional jump - has fallthrough
                            boundaries.push(instruction.offset + instruction.size as u32);
                        }
                    }
                }

                // Call instructions create blocks after the call
                OpCode::CALL | OpCode::CALL_L => {
                    if let Some(target_offset) = self.extract_jump_target(instruction) {
                        jump_targets.insert(target_offset);
                        boundaries.push(target_offset);
                    }
                    boundaries.push(instruction.offset + instruction.size as u32);
                }

                OpCode::CALLA => {
                    // Dynamic call - creates boundary after call
                    boundaries.push(instruction.offset + instruction.size as u32);
                }

                OpCode::CALLT => {
                    // Call token - creates boundary after call
                    boundaries.push(instruction.offset + instruction.size as u32);
                }

                // Try-catch blocks create boundaries
                OpCode::TRY | OpCode::TRY_L => {
                    if let Some(Operand::TryBlock {
                        catch_offset,
                        finally_offset,
                    }) = &instruction.operand
                    {
                        boundaries.push(*catch_offset);
                        if let Some(finally) = finally_offset {
                            boundaries.push(*finally);
                        }
                        boundaries.push(instruction.offset + instruction.size as u32);
                    }
                }

                // End of exception handling blocks
                OpCode::ENDTRY | OpCode::ENDTRY_L | OpCode::ENDFINALLY => {
                    boundaries.push(instruction.offset + instruction.size as u32);
                }

                // Return and abort end blocks (no fallthrough)
                OpCode::RET | OpCode::ABORT | OpCode::ABORTMSG | OpCode::THROW => {
                    boundaries.push(instruction.offset + instruction.size as u32);
                }

                _ => {}
            }
        }

        // Second pass: ensure all jump targets that point to valid instructions are boundaries
        for &target in &jump_targets {
            if instructions.iter().any(|instr| instr.offset == target) {
                boundaries.push(target);
            }
        }

        // Remove duplicates, sort, and filter out invalid boundaries
        boundaries.sort_unstable();
        boundaries.dedup();

        // Filter out boundaries that are beyond the last instruction
        if let Some(last_instr) = instructions.last() {
            let max_offset = last_instr.offset + last_instr.size as u32;
            boundaries.retain(|&offset| offset <= max_offset);
        }

        boundaries
    }

    /// Extract jump target offset from instruction
    fn extract_jump_target(&self, instruction: &Instruction) -> Option<u32> {
        match &instruction.operand {
            Some(Operand::JumpTarget(target)) => Some((instruction.offset as i32 + *target) as u32),
            Some(Operand::JumpTarget8(target)) => {
                Some((instruction.offset as i32 + *target as i32) as u32)
            }
            Some(Operand::JumpTarget32(target)) => {
                Some((instruction.offset as i32 + *target) as u32)
            }
            _ => None,
        }
    }

    /// Build basic blocks from instructions
    fn build_basic_blocks(
        &mut self,
        instructions: &[Instruction],
        boundaries: &[u32],
    ) -> Result<Vec<IRBlock>, LiftError> {
        let mut blocks = Vec::new();

        for (block_id, &start_offset) in boundaries.iter().enumerate() {
            let end_offset = boundaries.get(block_id + 1).copied().unwrap_or(u32::MAX);

            let block_instructions: Vec<_> = instructions
                .iter()
                .filter(|instr| instr.offset >= start_offset && instr.offset < end_offset)
                .collect();

            if !block_instructions.is_empty() {
                let mut block = self.build_block(block_id as u32, &block_instructions)?;

                // Build predecessor/successor relationships
                self.build_block_relationships(&mut block, &block_instructions, boundaries);

                blocks.push(block);
            }
        }

        Ok(blocks)
    }

    /// Build predecessor and successor relationships for a block
    fn build_block_relationships(
        &self,
        block: &mut IRBlock,
        instructions: &[&Instruction],
        boundaries: &[u32],
    ) {
        if let Some(&last_instruction) = instructions.last() {
            match &block.terminator {
                Terminator::Jump(target_id) => {
                    block.successors.push(*target_id);
                }
                Terminator::Branch {
                    true_target,
                    false_target,
                    ..
                } => {
                    block.successors.push(*true_target);
                    block.successors.push(*false_target);
                }
                Terminator::Switch {
                    targets,
                    default_target,
                    ..
                } => {
                    for (_, target_id) in targets {
                        block.successors.push(*target_id);
                    }
                    if let Some(default) = default_target {
                        block.successors.push(*default);
                    }
                }
                Terminator::TryBlock {
                    try_block,
                    catch_block,
                    finally_block,
                } => {
                    block.successors.push(*try_block);
                    if let Some(catch) = catch_block {
                        block.successors.push(*catch);
                    }
                    if let Some(finally) = finally_block {
                        block.successors.push(*finally);
                    }
                }
                Terminator::Return(_) | Terminator::Abort(_) => {
                    // No successors for terminal blocks
                }
            }

            // For fallthrough blocks (no explicit terminator), add next block as successor
            if matches!(block.terminator, Terminator::Jump(0))
                && !matches!(
                    last_instruction.opcode,
                    OpCode::RET
                        | OpCode::ABORT
                        | OpCode::ABORTMSG
                        | OpCode::THROW
                        | OpCode::JMP
                        | OpCode::JMP_L
                )
            {
                let next_offset = last_instruction.offset + last_instruction.size as u32;
                if let Some(&next_block_id) = self.offset_to_block.get(&next_offset) {
                    block.successors.clear();
                    block.successors.push(next_block_id);
                    block.terminator = Terminator::Jump(next_block_id);
                }
            }
        }

        // Remove duplicate successors
        block.successors.sort_unstable();
        block.successors.dedup();
    }

    /// Build predecessor relationships across all blocks
    fn build_predecessor_relationships(&self, blocks: &mut [IRBlock]) {
        // Create a map for quick lookup
        let mut predecessor_map: HashMap<BlockId, Vec<BlockId>> = HashMap::new();

        // Build predecessor map from successor relationships
        for block in blocks.iter() {
            for &successor_id in &block.successors {
                predecessor_map
                    .entry(successor_id)
                    .or_insert_with(Vec::new)
                    .push(block.id);
            }
        }

        // Set predecessor relationships
        for block in blocks.iter_mut() {
            if let Some(predecessors) = predecessor_map.get(&block.id) {
                block.predecessors = predecessors.clone();
                block.predecessors.sort_unstable();
                block.predecessors.dedup();
            }
        }
    }

    /// Validate that offset-to-block mapping is complete and correct
    fn validate_offset_mapping(&self, instructions: &[Instruction]) -> Result<(), LiftError> {
        // Check that all instruction offsets have valid block mappings
        for instruction in instructions {
            if !self.offset_to_block.contains_key(&instruction.offset) {
                return Err(LiftError::InvalidControlFlow {
                    offset: instruction.offset,
                });
            }

            // Validate that the block ID is reasonable
            let block_id = self.offset_to_block[&instruction.offset];
            if block_id >= 1000 {
                // Sanity check for block count
                return Err(LiftError::InvalidControlFlow {
                    offset: instruction.offset,
                });
            }
        }

        // Validate that jump targets point to valid blocks
        for instruction in instructions {
            if let Some(target_offset) = self.extract_jump_target(instruction) {
                if !self.offset_to_block.contains_key(&target_offset) {
                    // Check if target points to a valid instruction
                    if !instructions
                        .iter()
                        .any(|instr| instr.offset == target_offset)
                    {
                        return Err(LiftError::InvalidControlFlow {
                            offset: instruction.offset,
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Build single basic block from instructions
    fn build_block(
        &mut self,
        block_id: BlockId,
        instructions: &[&Instruction],
    ) -> Result<IRBlock, LiftError> {
        let mut block = IRBlock::new(block_id);

        // Process each instruction
        for instruction in instructions {
            let operations = self.lift_instruction(instruction)?;
            for operation in operations {
                block.add_operation(operation);
            }
        }

        // Set block terminator based on last instruction
        if let Some(&last_instruction) = instructions.last() {
            let terminator = self.build_terminator(last_instruction)?;
            block.set_terminator(terminator);
        }

        Ok(block)
    }

    /// Convert single instruction to IR operations
    fn lift_instruction(&mut self, instruction: &Instruction) -> Result<Vec<Operation>, LiftError> {
        self.current_offset = instruction.offset;
        self.update_max_stack_depth();

        let mut operations = Vec::new();

        // Skip terminator instructions - they are handled separately in build_terminator
        match instruction.opcode {
            OpCode::RET
            | OpCode::ABORT
            | OpCode::ABORTMSG
            | OpCode::THROW
            | OpCode::JMP
            | OpCode::JMP_L
            | OpCode::JMPIF
            | OpCode::JMPIF_L
            | OpCode::JMPIFNOT
            | OpCode::JMPIFNOT_L
            | OpCode::JMPEQ
            | OpCode::JMPEQ_L
            | OpCode::JMPNE
            | OpCode::JMPNE_L
            | OpCode::JMPGT
            | OpCode::JMPGT_L
            | OpCode::JMPGE
            | OpCode::JMPGE_L
            | OpCode::JMPLT
            | OpCode::JMPLT_L
            | OpCode::JMPLE
            | OpCode::JMPLE_L
            | OpCode::CALL
            | OpCode::CALL_L
            | OpCode::CALLA
            | OpCode::CALLT => {
                // These are terminators, handled separately
                return Ok(operations);
            }
            _ => {} // Continue with regular instruction processing
        }

        match instruction.opcode {
            // Constants - Push operations
            OpCode::PUSHINT8
            | OpCode::PUSHINT16
            | OpCode::PUSHINT32
            | OpCode::PUSHINT64
            | OpCode::PUSHINT128
            | OpCode::PUSHINT256 => {
                operations.extend(self.lift_push_constant(instruction)?);
            }

            OpCode::PUSHM1 => self.push_stack(Expression::Literal(Literal::Integer(-1))),
            OpCode::PUSH0 => self.push_stack(Expression::Literal(Literal::Integer(0))),
            OpCode::PUSH1 => self.push_stack(Expression::Literal(Literal::Integer(1))),
            OpCode::PUSH2 => self.push_stack(Expression::Literal(Literal::Integer(2))),
            OpCode::PUSH3 => self.push_stack(Expression::Literal(Literal::Integer(3))),
            OpCode::PUSH4 => self.push_stack(Expression::Literal(Literal::Integer(4))),
            OpCode::PUSH5 => self.push_stack(Expression::Literal(Literal::Integer(5))),
            OpCode::PUSH6 => self.push_stack(Expression::Literal(Literal::Integer(6))),
            OpCode::PUSH7 => self.push_stack(Expression::Literal(Literal::Integer(7))),
            OpCode::PUSH8 => self.push_stack(Expression::Literal(Literal::Integer(8))),
            OpCode::PUSH9 => self.push_stack(Expression::Literal(Literal::Integer(9))),
            OpCode::PUSH10 => self.push_stack(Expression::Literal(Literal::Integer(10))),
            OpCode::PUSH11 => self.push_stack(Expression::Literal(Literal::Integer(11))),
            OpCode::PUSH12 => self.push_stack(Expression::Literal(Literal::Integer(12))),
            OpCode::PUSH13 => self.push_stack(Expression::Literal(Literal::Integer(13))),
            OpCode::PUSH14 => self.push_stack(Expression::Literal(Literal::Integer(14))),
            OpCode::PUSH15 => self.push_stack(Expression::Literal(Literal::Integer(15))),
            OpCode::PUSH16 => self.push_stack(Expression::Literal(Literal::Integer(16))),

            OpCode::PUSHT => self.push_stack(Expression::Literal(Literal::Boolean(true))),
            OpCode::PUSHF => self.push_stack(Expression::Literal(Literal::Boolean(false))),
            OpCode::PUSHNULL => self.push_stack(Expression::Literal(Literal::Null)),

            OpCode::PUSHDATA1 | OpCode::PUSHDATA2 | OpCode::PUSHDATA4 => {
                operations.extend(self.lift_push_data(instruction)?);
            }

            OpCode::PUSHA => operations.extend(self.lift_push_address(instruction)?),

            // Flow control (handled in terminator)
            OpCode::NOP => operations.push(Operation::Comment("NOP".to_string())),

            // Stack operations
            OpCode::DEPTH => self.lift_stack_depth(),
            OpCode::DROP => {
                self.pop_stack()?;
            }
            OpCode::NIP => operations.extend(self.lift_nip()?),
            OpCode::XDROP => operations.extend(self.lift_xdrop(instruction)?),
            OpCode::CLEAR => self.stack.clear(),
            OpCode::DUP => operations.extend(self.lift_dup()?),
            OpCode::OVER => operations.extend(self.lift_over()?),
            OpCode::PICK => operations.extend(self.lift_pick(instruction)?),
            OpCode::TUCK => operations.extend(self.lift_tuck()?),
            OpCode::SWAP => operations.extend(self.lift_swap()?),
            OpCode::ROT => operations.extend(self.lift_rot()?),
            OpCode::ROLL => operations.extend(self.lift_roll(instruction)?),
            OpCode::REVERSE3 => operations.extend(self.lift_reverse3()?),
            OpCode::REVERSE4 => operations.extend(self.lift_reverse4()?),
            OpCode::REVERSEN => operations.extend(self.lift_reversen(instruction)?),

            // Arithmetic operations
            OpCode::ADD => operations.extend(self.lift_binary_arithmetic(BinaryOperator::Add)?),
            OpCode::SUB => operations.extend(self.lift_binary_arithmetic(BinaryOperator::Sub)?),
            OpCode::MUL => operations.extend(self.lift_binary_arithmetic(BinaryOperator::Mul)?),
            OpCode::DIV => operations.extend(self.lift_binary_arithmetic(BinaryOperator::Div)?),
            OpCode::MOD => operations.extend(self.lift_binary_arithmetic(BinaryOperator::Mod)?),
            OpCode::POW => operations.extend(self.lift_binary_arithmetic(BinaryOperator::Pow)?),

            // Unary arithmetic
            OpCode::SQRT => operations.extend(self.lift_unary_op(UnaryOperator::Sqrt)?),
            OpCode::ABS => operations.extend(self.lift_unary_op(UnaryOperator::Abs)?),
            OpCode::NEGATE => operations.extend(self.lift_unary_op(UnaryOperator::Negate)?),
            OpCode::INC => operations.extend(self.lift_increment(1)?),
            OpCode::DEC => operations.extend(self.lift_increment(-1)?),
            OpCode::SIGN => operations.extend(self.lift_unary_op(UnaryOperator::Sign)?),

            // Bitwise operations
            OpCode::AND => operations.extend(self.lift_binary_arithmetic(BinaryOperator::And)?),
            OpCode::OR => operations.extend(self.lift_binary_arithmetic(BinaryOperator::Or)?),
            OpCode::XOR => operations.extend(self.lift_binary_arithmetic(BinaryOperator::Xor)?),
            OpCode::INVERT => operations.extend(self.lift_unary_op(UnaryOperator::Not)?),
            OpCode::SHL => operations.extend(self.lift_shift(true)?),
            OpCode::SHR => operations.extend(self.lift_shift(false)?),

            // Comparison operations
            OpCode::EQUAL | OpCode::NUMEQUAL => {
                operations.extend(self.lift_binary_arithmetic(BinaryOperator::Equal)?)
            }
            OpCode::NOTEQUAL | OpCode::NUMNOTEQUAL => {
                operations.extend(self.lift_binary_arithmetic(BinaryOperator::NotEqual)?)
            }
            OpCode::LT => operations.extend(self.lift_binary_arithmetic(BinaryOperator::Less)?),
            OpCode::LE => {
                operations.extend(self.lift_binary_arithmetic(BinaryOperator::LessEqual)?)
            }
            OpCode::GT => operations.extend(self.lift_binary_arithmetic(BinaryOperator::Greater)?),
            OpCode::GE => {
                operations.extend(self.lift_binary_arithmetic(BinaryOperator::GreaterEqual)?)
            }

            // Boolean operations
            OpCode::NOT => operations.extend(self.lift_unary_op(UnaryOperator::BoolNot)?),
            OpCode::BOOLAND => {
                operations.extend(self.lift_binary_arithmetic(BinaryOperator::BoolAnd)?)
            }
            OpCode::BOOLOR => {
                operations.extend(self.lift_binary_arithmetic(BinaryOperator::BoolOr)?)
            }
            OpCode::NZ => operations.extend(self.lift_not_zero()?),

            // Min/Max operations
            OpCode::MIN => operations.extend(self.lift_builtin_call("min", 2)?),
            OpCode::MAX => operations.extend(self.lift_builtin_call("max", 2)?),
            OpCode::WITHIN => operations.extend(self.lift_builtin_call("within", 3)?),

            // Local/argument operations
            OpCode::LDLOC0 => operations.extend(self.lift_load_local(0)?),
            OpCode::LDLOC1 => operations.extend(self.lift_load_local(1)?),
            OpCode::LDLOC2 => operations.extend(self.lift_load_local(2)?),
            OpCode::LDLOC3 => operations.extend(self.lift_load_local(3)?),
            OpCode::LDLOC4 => operations.extend(self.lift_load_local(4)?),
            OpCode::LDLOC5 => operations.extend(self.lift_load_local(5)?),
            OpCode::LDLOC6 => operations.extend(self.lift_load_local(6)?),
            OpCode::LDLOC => operations.extend(self.lift_load_local_operand(instruction)?),
            OpCode::STLOC => operations.extend(self.lift_store_local_operand(instruction)?),

            OpCode::LDARG0 => operations.extend(self.lift_load_arg(0)?),
            OpCode::LDARG1 => operations.extend(self.lift_load_arg(1)?),
            OpCode::LDARG2 => operations.extend(self.lift_load_arg(2)?),
            OpCode::LDARG3 => operations.extend(self.lift_load_arg(3)?),
            OpCode::LDARG4 => operations.extend(self.lift_load_arg(4)?),
            OpCode::LDARG5 => operations.extend(self.lift_load_arg(5)?),
            OpCode::LDARG6 => operations.extend(self.lift_load_arg(6)?),
            OpCode::LDARG => operations.extend(self.lift_load_arg_operand(instruction)?),
            OpCode::STARG => operations.extend(self.lift_store_arg_operand(instruction)?),

            // Static field operations
            OpCode::LDSFLD0 => operations.extend(self.lift_load_static(0)?),
            OpCode::LDSFLD1 => operations.extend(self.lift_load_static(1)?),
            OpCode::LDSFLD2 => operations.extend(self.lift_load_static(2)?),
            OpCode::LDSFLD3 => operations.extend(self.lift_load_static(3)?),
            OpCode::LDSFLD4 => operations.extend(self.lift_load_static(4)?),
            OpCode::LDSFLD5 => operations.extend(self.lift_load_static(5)?),
            OpCode::LDSFLD6 => operations.extend(self.lift_load_static(6)?),
            OpCode::LDSFLD => operations.extend(self.lift_load_static_operand(instruction)?),
            OpCode::STSFLD => operations.extend(self.lift_store_static_operand(instruction)?),

            // Slot initialization
            OpCode::INITSLOT => operations.extend(self.lift_init_slot(instruction)?),
            OpCode::INITSSLOT => operations.extend(self.lift_init_static_slot(instruction)?),

            // Array/collection operations
            OpCode::NEWARRAY0 => operations.extend(self.lift_new_array_zero()?),
            OpCode::NEWARRAY | OpCode::NEWARRAYT => operations.extend(self.lift_new_array()?),
            OpCode::NEWSTRUCT0 => operations.extend(self.lift_new_struct_zero()?),
            OpCode::NEWSTRUCT => operations.extend(self.lift_new_struct()?),
            OpCode::NEWMAP => operations.extend(self.lift_new_map()?),
            OpCode::NEWBUFFER => operations.extend(self.lift_new_buffer(instruction)?),

            OpCode::APPEND => operations.extend(self.lift_array_append()?),
            OpCode::SETITEM => operations.extend(self.lift_array_set_item()?),
            OpCode::PICKITEM => operations.extend(self.lift_array_pick_item()?),
            OpCode::REMOVE => operations.extend(self.lift_array_remove()?),
            OpCode::SIZE => operations.extend(self.lift_array_size()?),
            OpCode::CLEARITEMS => operations.extend(self.lift_array_clear()?),
            OpCode::HASKEY => operations.extend(self.lift_map_has_key()?),
            OpCode::KEYS => operations.extend(self.lift_map_keys()?),
            OpCode::VALUES => operations.extend(self.lift_map_values()?),

            // String operations
            OpCode::CAT => operations.extend(self.lift_string_concat()?),
            OpCode::SUBSTR => operations.extend(self.lift_string_substring()?),
            OpCode::LEFT => operations.extend(self.lift_string_left()?),
            OpCode::RIGHT => operations.extend(self.lift_string_right()?),

            // Pack/unpack operations
            OpCode::PACKARRAY => operations.extend(self.lift_pack(instruction)?),
            OpCode::PACKSTRUCT => operations.extend(self.lift_pack(instruction)?),
            OpCode::PACKMAP => operations.extend(self.lift_pack(instruction)?),
            OpCode::UNPACK => operations.extend(self.lift_unpack()?),

            // Type operations
            OpCode::ISNULL => {
                operations.extend(self.lift_type_check(Type::Primitive(PrimitiveType::Null))?)
            }
            OpCode::ISTYPE => operations.extend(self.lift_is_type(instruction)?),
            OpCode::CONVERT => operations.extend(self.lift_convert(instruction)?),

            // Syscalls
            OpCode::SYSCALL => operations.extend(self.lift_syscall(instruction)?),

            // Exception handling
            OpCode::THROW => operations.extend(self.lift_throw()?),
            OpCode::ASSERT => operations.extend(self.lift_assert(false)?),
            OpCode::ASSERTMSG => operations.extend(self.lift_assert(true)?),
            OpCode::ABORT => operations.extend(self.lift_abort(false)?),
            OpCode::ABORTMSG => operations.extend(self.lift_abort(true)?),

            // For unhandled opcodes, add a comment
            // Unknown opcodes - handle gracefully to avoid failures
            OpCode::UNKNOWN_07
            | OpCode::UNKNOWN_42
            | OpCode::UNKNOWN_44
            | OpCode::UNKNOWN_B6
            | OpCode::UNKNOWN_B7
            | OpCode::UNKNOWN_B8
            | OpCode::UNKNOWN_BB
            | OpCode::UNKNOWN_94
            | OpCode::UNKNOWN_DA
            | OpCode::UNKNOWN_E4
            | OpCode::UNKNOWN_E6
            | OpCode::UNKNOWN_E8
            | OpCode::UNKNOWN_E9
            | OpCode::UNKNOWN_EA
            | OpCode::UNKNOWN_EC
            | OpCode::UNKNOWN_EF
            | OpCode::UNKNOWN_F0
            | OpCode::UNKNOWN_F2
            | OpCode::UNKNOWN_F7
            | OpCode::UNKNOWN_FF => {
                operations.push(Operation::Comment(format!(
                    "TODO: Implement opcode {:?} at offset {}",
                    instruction.opcode, instruction.offset
                )));
                // For now, assume these are stack-neutral operations
            }

            _ => {
                operations.push(Operation::Comment(format!(
                    "Unhandled opcode: {:?} at offset {}",
                    instruction.opcode, instruction.offset
                )));
            }
        }

        Ok(operations)
    }

    /// Build terminator for block
    fn build_terminator(&mut self, instruction: &Instruction) -> Result<Terminator, LiftError> {
        match instruction.opcode {
            OpCode::RET => {
                // Check if there's a return value on the stack
                let return_value = if !self.stack.is_empty() {
                    Some(
                        self.pop_stack()
                            .unwrap_or(Expression::Literal(Literal::Null)),
                    )
                } else {
                    None
                };
                Ok(Terminator::Return(return_value))
            }

            OpCode::ABORT => Ok(Terminator::Abort(None)),
            OpCode::ABORTMSG => {
                let message = if !self.stack.is_empty() {
                    Some(
                        self.pop_stack()
                            .unwrap_or(Expression::Literal(Literal::Null)),
                    )
                } else {
                    None
                };
                Ok(Terminator::Abort(message))
            }

            OpCode::THROW => {
                let exception = if !self.stack.is_empty() {
                    self.pop_stack()
                        .unwrap_or(Expression::Literal(Literal::Null))
                } else {
                    Expression::Literal(Literal::Null)
                };
                Ok(Terminator::Abort(Some(exception)))
            }

            // Unconditional jumps
            OpCode::JMP | OpCode::JMP_L => {
                if let Some(target_offset) = self.extract_jump_target(instruction) {
                    let target_block = self
                        .offset_to_block
                        .get(&target_offset)
                        .copied()
                        .unwrap_or(0);
                    Ok(Terminator::Jump(target_block))
                } else {
                    Err(LiftError::InvalidControlFlow {
                        offset: instruction.offset,
                    })
                }
            }

            // Conditional jumps
            OpCode::JMPIF | OpCode::JMPIF_L => {
                if let Some(target_offset) = self.extract_jump_target(instruction) {
                    let true_target = self
                        .offset_to_block
                        .get(&target_offset)
                        .copied()
                        .unwrap_or(0);
                    let false_target = self
                        .offset_to_block
                        .get(&(instruction.offset + instruction.size as u32))
                        .copied()
                        .unwrap_or(0);

                    let condition = if !self.stack.is_empty() {
                        self.pop_stack()
                            .unwrap_or(Expression::Literal(Literal::Boolean(true)))
                    } else {
                        Expression::Literal(Literal::Boolean(true))
                    };

                    Ok(Terminator::Branch {
                        condition,
                        true_target,
                        false_target,
                    })
                } else {
                    Err(LiftError::InvalidControlFlow {
                        offset: instruction.offset,
                    })
                }
            }

            OpCode::JMPIFNOT | OpCode::JMPIFNOT_L => {
                if let Some(target_offset) = self.extract_jump_target(instruction) {
                    let true_target = self
                        .offset_to_block
                        .get(&(instruction.offset + instruction.size as u32))
                        .copied()
                        .unwrap_or(0);
                    let false_target = self
                        .offset_to_block
                        .get(&target_offset)
                        .copied()
                        .unwrap_or(0);

                    let condition = if !self.stack.is_empty() {
                        self.pop_stack()
                            .unwrap_or(Expression::Literal(Literal::Boolean(true)))
                    } else {
                        Expression::Literal(Literal::Boolean(true))
                    };

                    Ok(Terminator::Branch {
                        condition,
                        true_target,
                        false_target,
                    })
                } else {
                    Err(LiftError::InvalidControlFlow {
                        offset: instruction.offset,
                    })
                }
            }

            // Comparison-based jumps
            OpCode::JMPEQ
            | OpCode::JMPEQ_L
            | OpCode::JMPNE
            | OpCode::JMPNE_L
            | OpCode::JMPGT
            | OpCode::JMPGT_L
            | OpCode::JMPGE
            | OpCode::JMPGE_L
            | OpCode::JMPLT
            | OpCode::JMPLT_L
            | OpCode::JMPLE
            | OpCode::JMPLE_L => {
                if let Some(target_offset) = self.extract_jump_target(instruction) {
                    let true_target = self
                        .offset_to_block
                        .get(&target_offset)
                        .copied()
                        .unwrap_or(0);
                    let false_target = self
                        .offset_to_block
                        .get(&(instruction.offset + instruction.size as u32))
                        .copied()
                        .unwrap_or(0);

                    // Pop comparison operands and create comparison condition
                    let right = if !self.stack.is_empty() {
                        self.pop_stack()
                            .unwrap_or(Expression::Literal(Literal::Integer(0)))
                    } else {
                        Expression::Literal(Literal::Integer(0))
                    };

                    let left = if !self.stack.is_empty() {
                        self.pop_stack()
                            .unwrap_or(Expression::Literal(Literal::Integer(0)))
                    } else {
                        Expression::Literal(Literal::Integer(0))
                    };

                    let operator = match instruction.opcode {
                        OpCode::JMPEQ | OpCode::JMPEQ_L => BinaryOperator::Equal,
                        OpCode::JMPNE | OpCode::JMPNE_L => BinaryOperator::NotEqual,
                        OpCode::JMPGT | OpCode::JMPGT_L => BinaryOperator::Greater,
                        OpCode::JMPGE | OpCode::JMPGE_L => BinaryOperator::GreaterEqual,
                        OpCode::JMPLT | OpCode::JMPLT_L => BinaryOperator::Less,
                        OpCode::JMPLE | OpCode::JMPLE_L => BinaryOperator::LessEqual,
                        _ => BinaryOperator::Equal,
                    };

                    let condition = Expression::BinaryOp {
                        op: operator,
                        left: Box::new(left),
                        right: Box::new(right),
                    };

                    Ok(Terminator::Branch {
                        condition,
                        true_target,
                        false_target,
                    })
                } else {
                    Err(LiftError::InvalidControlFlow {
                        offset: instruction.offset,
                    })
                }
            }

            // Function calls
            OpCode::CALL | OpCode::CALL_L => {
                if let Some(target_offset) = self.extract_jump_target(instruction) {
                    let target_block = self
                        .offset_to_block
                        .get(&target_offset)
                        .copied()
                        .unwrap_or(0);
                    // After call, continue to next instruction
                    let return_block = self
                        .offset_to_block
                        .get(&(instruction.offset + instruction.size as u32))
                        .copied()
                        .unwrap_or(0);
                    Ok(Terminator::Jump(return_block))
                } else {
                    Err(LiftError::InvalidControlFlow {
                        offset: instruction.offset,
                    })
                }
            }

            OpCode::CALLA | OpCode::CALLT => {
                // Dynamic calls - continue to next instruction
                let next_block = self
                    .offset_to_block
                    .get(&(instruction.offset + instruction.size as u32))
                    .copied()
                    .unwrap_or(0);
                Ok(Terminator::Jump(next_block))
            }

            // Try-catch constructs
            OpCode::TRY | OpCode::TRY_L => {
                if let Some(Operand::TryBlock {
                    catch_offset,
                    finally_offset,
                }) = &instruction.operand
                {
                    let try_block = self
                        .offset_to_block
                        .get(&(instruction.offset + instruction.size as u32))
                        .copied()
                        .unwrap_or(0);
                    let catch_block = self.offset_to_block.get(catch_offset).copied();
                    let finally_block = finally_offset
                        .and_then(|offset| self.offset_to_block.get(&offset).copied());

                    Ok(Terminator::TryBlock {
                        try_block,
                        catch_block,
                        finally_block,
                    })
                } else {
                    Err(LiftError::InvalidControlFlow {
                        offset: instruction.offset,
                    })
                }
            }

            // Default fallthrough to next block
            _ => {
                let next_offset = instruction.offset + instruction.size as u32;
                let next_block = self.offset_to_block.get(&next_offset).copied().unwrap_or(0);
                Ok(Terminator::Jump(next_block))
            }
        }
    }

    // ============= Stack Management =============

    /// Push value onto stack simulation with depth tracking
    fn push_stack(&mut self, expr: Expression) {
        self.stack.push(expr);
        self.update_max_stack_depth();
    }

    /// Pop value from stack simulation with error context
    fn pop_stack(&mut self) -> Result<Expression, LiftError> {
        self.stack.pop().ok_or(LiftError::StackUnderflow {
            offset: self.current_offset,
        })
    }

    /// Peek at stack item without removing it
    fn peek_stack(&self, depth: usize) -> Result<Expression, LiftError> {
        let stack_len = self.stack.len();
        if depth >= stack_len {
            return Err(LiftError::StackUnderflow {
                offset: self.current_offset,
            });
        }
        Ok(self.stack[stack_len - 1 - depth].clone())
    }

    /// Update maximum stack depth reached
    fn update_max_stack_depth(&mut self) {
        self.max_stack_depth = self.max_stack_depth.max(self.stack.len());
    }

    /// Validate stack has at least n items
    fn validate_stack_depth(&self, required: usize) -> Result<(), LiftError> {
        if self.stack.len() < required {
            Err(LiftError::StackUnderflow {
                offset: self.current_offset,
            })
        } else {
            Ok(())
        }
    }

    // ============= Variable Management =============

    /// Create temporary variable with type
    fn create_temp_variable(&mut self, var_type: Type) -> Variable {
        let id = self.var_counter;
        self.var_counter += 1;

        Variable {
            name: format!("temp_{}", id),
            id,
            var_type: VariableType::Temporary,
        }
    }

    /// Get or create local variable
    fn get_or_create_local(&mut self, slot: u32) -> Variable {
        if let Some(var) = self.variables.get(&slot) {
            var.clone()
        } else {
            let var = Variable {
                name: format!("local_{}", slot),
                id: self.var_counter,
                var_type: VariableType::Local,
            };
            self.var_counter += 1;
            self.variables.insert(slot, var.clone());
            var
        }
    }

    /// Get or create argument variable
    fn get_or_create_arg(&mut self, slot: u32) -> Variable {
        if let Some(var) = self.arguments.get(&slot) {
            var.clone()
        } else {
            let var = Variable {
                name: format!("arg_{}", slot),
                id: self.var_counter,
                var_type: VariableType::Parameter,
            };
            self.var_counter += 1;
            self.arguments.insert(slot, var.clone());
            var
        }
    }

    /// Get or create static field variable
    fn get_or_create_static(&mut self, slot: u32) -> Variable {
        if let Some(var) = self.static_fields.get(&slot) {
            var.clone()
        } else {
            let var = Variable {
                name: format!("static_{}", slot),
                id: self.var_counter,
                var_type: VariableType::Static,
            };
            self.var_counter += 1;
            self.static_fields.insert(slot, var.clone());
            var
        }
    }

    // ============= Block and Control Flow =============

    /// Convert offset and target to block ID using mapping
    fn offset_to_block_id(&self, current_offset: u32, target_offset: i32) -> BlockId {
        let absolute_target = (current_offset as i32 + target_offset) as u32;
        *self.offset_to_block.get(&absolute_target).unwrap_or(&0)
    }

    // ============= Syscall Resolution =============

    /// Resolve syscall hash to name using syscall database
    fn resolve_syscall_name(&self, hash: u32) -> String {
        self.syscall_db.resolve_name(hash)
    }

    /// Get syscall argument count from hash
    fn get_syscall_arg_count(&self, hash: u32) -> usize {
        self.syscall_db.get_arg_count(hash)
    }

    /// Check if syscall returns a value from hash
    fn syscall_returns_value(&self, hash: u32) -> bool {
        self.syscall_db.returns_value(hash)
    }

    // ============= Type Conversion =============

    /// Convert StackItemType to Type
    fn stack_item_type_to_type(&self, stack_type: StackItemType) -> Type {
        match stack_type {
            StackItemType::Boolean => Type::Primitive(PrimitiveType::Boolean),
            StackItemType::Integer => Type::Primitive(PrimitiveType::Integer),
            StackItemType::ByteString => Type::Primitive(PrimitiveType::ByteString),
            StackItemType::Buffer => Type::Buffer,
            StackItemType::Array => Type::Array(Box::new(Type::Unknown)),
            StackItemType::Struct => Type::Struct(StructType {
                name: None,
                fields: Vec::new(),
                is_packed: false,
            }),
            StackItemType::Map => Type::Map {
                key: Box::new(Type::Unknown),
                value: Box::new(Type::Unknown),
            },
            StackItemType::InteropInterface => Type::InteropInterface("Unknown".to_string()),
            StackItemType::Pointer => Type::Pointer(Box::new(Type::Unknown)),
            _ => Type::Unknown,
        }
    }

    // ============= Instruction Lifting Helpers =============

    /// Lift push constant operations
    fn lift_push_constant(
        &mut self,
        instruction: &Instruction,
    ) -> Result<Vec<Operation>, LiftError> {
        match &instruction.operand {
            Some(Operand::Integer(value)) => {
                self.push_stack(Expression::Literal(Literal::Integer(*value)));
            }
            Some(Operand::BigInteger(bytes)) => {
                self.push_stack(Expression::Literal(Literal::ByteArray(bytes.clone())));
            }
            _ => {
                return Err(LiftError::InvalidOperand {
                    offset: instruction.offset,
                })
            }
        }
        Ok(vec![])
    }

    /// Lift push data operations
    fn lift_push_data(&mut self, instruction: &Instruction) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::Bytes(bytes)) = &instruction.operand {
            self.push_stack(Expression::Literal(Literal::ByteArray(bytes.clone())));
        }
        Ok(vec![])
    }

    /// Lift push address operations
    fn lift_push_address(
        &mut self,
        instruction: &Instruction,
    ) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::JumpTarget32(offset)) = &instruction.operand {
            let target_offset = (instruction.offset as i32 + *offset) as u32;
            self.push_stack(Expression::Literal(Literal::Integer(target_offset as i64)));
        }
        Ok(vec![])
    }

    /// Lift stack depth operation
    fn lift_stack_depth(&mut self) {
        let depth = self.stack.len() as i64;
        self.push_stack(Expression::Literal(Literal::Integer(depth)));
    }

    /// Lift NIP operation (remove second item)
    fn lift_nip(&mut self) -> Result<Vec<Operation>, LiftError> {
        let top = self.pop_stack()?;
        self.pop_stack()?; // Drop second item
        self.push_stack(top);
        Ok(vec![])
    }

    /// Lift XDROP operation
    fn lift_xdrop(&mut self, instruction: &Instruction) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::Count(n)) = &instruction.operand {
            for _ in 0..*n {
                self.pop_stack()?;
            }
        }
        Ok(vec![])
    }

    /// Lift DUP operation
    fn lift_dup(&mut self) -> Result<Vec<Operation>, LiftError> {
        let top = self.peek_stack(0)?;
        self.push_stack(top);
        Ok(vec![])
    }

    /// Lift OVER operation
    fn lift_over(&mut self) -> Result<Vec<Operation>, LiftError> {
        let second = self.peek_stack(1)?;
        self.push_stack(second);
        Ok(vec![])
    }

    /// Lift PICK operation
    fn lift_pick(&mut self, instruction: &Instruction) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::Count(n)) = &instruction.operand {
            let value = self.peek_stack(*n as usize)?;
            self.push_stack(value);
        }
        Ok(vec![])
    }

    /// Lift TUCK operation
    fn lift_tuck(&mut self) -> Result<Vec<Operation>, LiftError> {
        let a = self.pop_stack()?;
        let b = self.pop_stack()?;
        self.push_stack(a.clone());
        self.push_stack(b);
        self.push_stack(a);
        Ok(vec![])
    }

    /// Lift SWAP operation
    fn lift_swap(&mut self) -> Result<Vec<Operation>, LiftError> {
        let a = self.pop_stack()?;
        let b = self.pop_stack()?;
        self.push_stack(a);
        self.push_stack(b);
        Ok(vec![])
    }

    /// Lift ROT operation
    fn lift_rot(&mut self) -> Result<Vec<Operation>, LiftError> {
        let a = self.pop_stack()?;
        let b = self.pop_stack()?;
        let c = self.pop_stack()?;
        self.push_stack(b);
        self.push_stack(a);
        self.push_stack(c);
        Ok(vec![])
    }

    /// Lift ROLL operation
    fn lift_roll(&mut self, instruction: &Instruction) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::Count(n)) = &instruction.operand {
            if *n > 0 {
                let mut items = Vec::new();
                for _ in 0..=*n {
                    items.push(self.pop_stack()?);
                }
                let rolled = items.pop().unwrap();
                for item in items.into_iter().rev() {
                    self.push_stack(item);
                }
                self.push_stack(rolled);
            }
        }
        Ok(vec![])
    }

    /// Lift REVERSE3 operation
    fn lift_reverse3(&mut self) -> Result<Vec<Operation>, LiftError> {
        let a = self.pop_stack()?;
        let b = self.pop_stack()?;
        let c = self.pop_stack()?;
        self.push_stack(a);
        self.push_stack(b);
        self.push_stack(c);
        Ok(vec![])
    }

    /// Lift REVERSE4 operation
    fn lift_reverse4(&mut self) -> Result<Vec<Operation>, LiftError> {
        let a = self.pop_stack()?;
        let b = self.pop_stack()?;
        let c = self.pop_stack()?;
        let d = self.pop_stack()?;
        self.push_stack(a);
        self.push_stack(b);
        self.push_stack(c);
        self.push_stack(d);
        Ok(vec![])
    }

    /// Lift REVERSEN operation
    fn lift_reversen(&mut self, instruction: &Instruction) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::Count(n)) = &instruction.operand {
            let mut items = Vec::new();
            for _ in 0..*n {
                items.push(self.pop_stack()?);
            }
            for item in items {
                self.push_stack(item);
            }
        }
        Ok(vec![])
    }

    /// Lift binary arithmetic operations
    fn lift_binary_arithmetic(
        &mut self,
        operator: BinaryOperator,
    ) -> Result<Vec<Operation>, LiftError> {
        self.validate_stack_depth(2)?;

        let right = self.pop_stack()?;
        let left = self.pop_stack()?;
        let target = self.create_temp_variable(Type::Unknown);

        let operation = Operation::Arithmetic {
            operator,
            left,
            right,
            target: target.clone(),
        };

        self.push_stack(Expression::Variable(target));
        Ok(vec![operation])
    }

    /// Lift unary operations
    fn lift_unary_op(&mut self, operator: UnaryOperator) -> Result<Vec<Operation>, LiftError> {
        let operand = self.pop_stack()?;
        let target = self.create_temp_variable(Type::Unknown);

        let operation = Operation::Unary {
            operator,
            operand,
            target: target.clone(),
        };

        self.push_stack(Expression::Variable(target));
        Ok(vec![operation])
    }

    /// Lift increment/decrement operations
    fn lift_increment(&mut self, delta: i64) -> Result<Vec<Operation>, LiftError> {
        let operand = self.pop_stack()?;
        let increment = Expression::Literal(Literal::Integer(delta));
        let target = self.create_temp_variable(Type::Unknown);

        let operation = Operation::Arithmetic {
            operator: if delta > 0 {
                BinaryOperator::Add
            } else {
                BinaryOperator::Sub
            },
            left: operand,
            right: increment,
            target: target.clone(),
        };

        self.push_stack(Expression::Variable(target));
        Ok(vec![operation])
    }

    /// Lift shift operations
    fn lift_shift(&mut self, left_shift: bool) -> Result<Vec<Operation>, LiftError> {
        let shift = self.pop_stack()?;
        let value = self.pop_stack()?;
        let target = self.create_temp_variable(Type::Unknown);

        let operator = if left_shift {
            BinaryOperator::LeftShift
        } else {
            BinaryOperator::RightShift
        };

        let operation = Operation::Arithmetic {
            operator,
            left: value,
            right: shift,
            target: target.clone(),
        };

        self.push_stack(Expression::Variable(target));
        Ok(vec![operation])
    }

    /// Lift not-zero operation
    fn lift_not_zero(&mut self) -> Result<Vec<Operation>, LiftError> {
        let operand = self.pop_stack()?;
        let zero = Expression::Literal(Literal::Integer(0));
        let target = self.create_temp_variable(Type::Unknown);

        let operation = Operation::Arithmetic {
            operator: BinaryOperator::NotEqual,
            left: operand,
            right: zero,
            target: target.clone(),
        };

        self.push_stack(Expression::Variable(target));
        Ok(vec![operation])
    }

    /// Lift builtin function calls
    fn lift_builtin_call(
        &mut self,
        name: &str,
        arg_count: usize,
    ) -> Result<Vec<Operation>, LiftError> {
        self.validate_stack_depth(arg_count)?;

        let mut arguments = Vec::new();
        for _ in 0..arg_count {
            arguments.push(self.pop_stack()?);
        }
        arguments.reverse(); // Arguments are pushed in reverse order

        let target = self.create_temp_variable(Type::Unknown);

        let operation = Operation::BuiltinCall {
            name: name.to_string(),
            arguments,
            target: Some(target.clone()),
        };

        self.push_stack(Expression::Variable(target));
        Ok(vec![operation])
    }

    /// Lift load local variable operations
    fn lift_load_local(&mut self, slot: u32) -> Result<Vec<Operation>, LiftError> {
        let var = self.get_or_create_local(slot);
        self.push_stack(Expression::Variable(var));
        Ok(vec![])
    }

    /// Lift load local with operand
    fn lift_load_local_operand(
        &mut self,
        instruction: &Instruction,
    ) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::SlotIndex(slot)) = &instruction.operand {
            self.lift_load_local(*slot as u32)
        } else {
            Err(LiftError::InvalidOperand {
                offset: instruction.offset,
            })
        }
    }

    /// Lift store local with operand
    fn lift_store_local_operand(
        &mut self,
        instruction: &Instruction,
    ) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::SlotIndex(slot)) = &instruction.operand {
            self.lift_store_local(*slot as u32)
        } else {
            Err(LiftError::InvalidOperand {
                offset: instruction.offset,
            })
        }
    }

    /// Lift store local variable operations
    fn lift_store_local(&mut self, slot: u32) -> Result<Vec<Operation>, LiftError> {
        let value = self.pop_stack()?;
        let target = self.get_or_create_local(slot);

        let operation = Operation::Assign {
            target,
            source: value,
        };

        Ok(vec![operation])
    }

    /// Lift load argument operations
    fn lift_load_arg(&mut self, slot: u32) -> Result<Vec<Operation>, LiftError> {
        let var = self.get_or_create_arg(slot);
        self.push_stack(Expression::Variable(var));
        Ok(vec![])
    }

    /// Lift load argument with operand
    fn lift_load_arg_operand(
        &mut self,
        instruction: &Instruction,
    ) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::SlotIndex(slot)) = &instruction.operand {
            self.lift_load_arg(*slot as u32)
        } else {
            Err(LiftError::InvalidOperand {
                offset: instruction.offset,
            })
        }
    }

    /// Lift store argument with operand
    fn lift_store_arg_operand(
        &mut self,
        instruction: &Instruction,
    ) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::SlotIndex(slot)) = &instruction.operand {
            self.lift_store_arg(*slot as u32)
        } else {
            Err(LiftError::InvalidOperand {
                offset: instruction.offset,
            })
        }
    }

    /// Lift store argument operations
    fn lift_store_arg(&mut self, slot: u32) -> Result<Vec<Operation>, LiftError> {
        let value = self.pop_stack()?;
        let target = self.get_or_create_arg(slot);

        let operation = Operation::Assign {
            target,
            source: value,
        };

        Ok(vec![operation])
    }

    /// Lift load static field operations
    fn lift_load_static(&mut self, slot: u32) -> Result<Vec<Operation>, LiftError> {
        let var = self.get_or_create_static(slot);
        self.push_stack(Expression::Variable(var));
        Ok(vec![])
    }

    /// Lift load static with operand
    fn lift_load_static_operand(
        &mut self,
        instruction: &Instruction,
    ) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::SlotIndex(slot)) = &instruction.operand {
            self.lift_load_static(*slot as u32)
        } else {
            Err(LiftError::InvalidOperand {
                offset: instruction.offset,
            })
        }
    }

    /// Lift store static with operand
    fn lift_store_static_operand(
        &mut self,
        instruction: &Instruction,
    ) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::SlotIndex(slot)) = &instruction.operand {
            self.lift_store_static(*slot as u32)
        } else {
            Err(LiftError::InvalidOperand {
                offset: instruction.offset,
            })
        }
    }

    /// Lift store static field operations
    fn lift_store_static(&mut self, slot: u32) -> Result<Vec<Operation>, LiftError> {
        let value = self.pop_stack()?;
        let target = self.get_or_create_static(slot);

        let operation = Operation::Assign {
            target,
            source: value,
        };

        Ok(vec![operation])
    }

    /// Lift slot initialization
    fn lift_init_slot(&mut self, instruction: &Instruction) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::SlotInit {
            local_slots,
            static_slots,
        }) = &instruction.operand
        {
            // In Neo N3, INITSLOT typically appears at method start
            // If the stack is empty but we're using STARG instructions later,
            // we need to simulate that arguments were passed to this method
            if self.stack.is_empty() {
                // Push placeholder arguments based on common usage patterns
                // This is a heuristic - in a full implementation, we'd use manifest info
                for i in 0..4 {
                    // Assume up to 4 arguments might be needed
                    let arg_var = Variable {
                        name: format!("arg_{}", i),
                        id: self.var_counter,
                        var_type: VariableType::Parameter,
                    };
                    self.var_counter += 1;
                    self.push_stack(Expression::Variable(arg_var));
                }
            }

            Ok(vec![Operation::Comment(format!(
                "Initialize {} local slots, {} static slots",
                local_slots, static_slots
            ))])
        } else {
            Ok(vec![])
        }
    }

    /// Lift static slot initialization
    fn lift_init_static_slot(
        &mut self,
        instruction: &Instruction,
    ) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::SlotInit { static_slots, .. }) = &instruction.operand {
            Ok(vec![Operation::Comment(format!(
                "Initialize {} static slots",
                static_slots
            ))])
        } else {
            Ok(vec![])
        }
    }

    /// Lift new array with zero elements
    fn lift_new_array_zero(&mut self) -> Result<Vec<Operation>, LiftError> {
        self.push_stack(Expression::ArrayCreate {
            element_type: Box::new(Type::Unknown),
            size: Box::new(Expression::Literal(Literal::Integer(0))),
        });
        Ok(vec![])
    }

    /// Lift new array operations
    fn lift_new_array(&mut self) -> Result<Vec<Operation>, LiftError> {
        let count = self.pop_stack()?;
        self.push_stack(Expression::ArrayCreate {
            element_type: Box::new(Type::Unknown),
            size: Box::new(count),
        });
        Ok(vec![])
    }

    /// Lift new struct with zero fields
    fn lift_new_struct_zero(&mut self) -> Result<Vec<Operation>, LiftError> {
        self.push_stack(Expression::StructCreate { fields: Vec::new() });
        Ok(vec![])
    }

    /// Lift new struct operations
    fn lift_new_struct(&mut self) -> Result<Vec<Operation>, LiftError> {
        let _count = self.pop_stack()?; // Count is used for validation
        self.push_stack(Expression::StructCreate {
            fields: Vec::new(), // Field information from struct definition
        });
        Ok(vec![])
    }

    /// Lift new map operations
    fn lift_new_map(&mut self) -> Result<Vec<Operation>, LiftError> {
        self.push_stack(Expression::MapCreate {
            entries: Vec::new(),
        });
        Ok(vec![])
    }

    /// Lift new buffer operations
    fn lift_new_buffer(&mut self, instruction: &Instruction) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::BufferSize(size)) = &instruction.operand {
            self.push_stack(Expression::Literal(Literal::ByteArray(vec![
                0;
                *size as usize
            ])));
        }
        Ok(vec![])
    }

    /// Lift array append operations
    fn lift_array_append(&mut self) -> Result<Vec<Operation>, LiftError> {
        let item = self.pop_stack()?;
        let array = self.pop_stack()?;
        let target = self.create_temp_variable(Type::Unknown);

        let operation = Operation::ArrayOp {
            operation: ArrayOperation::Append,
            array,
            index: None,
            value: Some(item),
            target: Some(target.clone()),
        };

        self.push_stack(Expression::Variable(target));
        Ok(vec![operation])
    }

    /// Lift array set item operations
    fn lift_array_set_item(&mut self) -> Result<Vec<Operation>, LiftError> {
        let value = self.pop_stack()?;
        let index = self.pop_stack()?;
        let array = self.pop_stack()?;

        let operation = Operation::ArrayOp {
            operation: ArrayOperation::SetItem,
            array,
            index: Some(index),
            value: Some(value),
            target: None,
        };

        Ok(vec![operation])
    }

    /// Lift array pick item operations
    fn lift_array_pick_item(&mut self) -> Result<Vec<Operation>, LiftError> {
        let index = self.pop_stack()?;
        let array = self.pop_stack()?;
        let target = self.create_temp_variable(Type::Unknown);

        let operation = Operation::ArrayOp {
            operation: ArrayOperation::GetItem,
            array,
            index: Some(index),
            value: None,
            target: Some(target.clone()),
        };

        self.push_stack(Expression::Variable(target));
        Ok(vec![operation])
    }

    /// Lift array remove operations
    fn lift_array_remove(&mut self) -> Result<Vec<Operation>, LiftError> {
        let index = self.pop_stack()?;
        let array = self.pop_stack()?;

        let operation = Operation::ArrayOp {
            operation: ArrayOperation::Remove,
            array,
            index: Some(index),
            value: None,
            target: None,
        };

        Ok(vec![operation])
    }

    /// Lift array size operations
    fn lift_array_size(&mut self) -> Result<Vec<Operation>, LiftError> {
        let array = self.pop_stack()?;
        let target = self.create_temp_variable(Type::Unknown);

        let operation = Operation::ArrayOp {
            operation: ArrayOperation::Size,
            array,
            index: None,
            value: None,
            target: Some(target.clone()),
        };

        self.push_stack(Expression::Variable(target));
        Ok(vec![operation])
    }

    /// Lift array clear operations
    fn lift_array_clear(&mut self) -> Result<Vec<Operation>, LiftError> {
        let array = self.pop_stack()?;

        let operation = Operation::ArrayOp {
            operation: ArrayOperation::Clear,
            array,
            index: None,
            value: None,
            target: None,
        };

        Ok(vec![operation])
    }

    /// Lift map has key operations
    fn lift_map_has_key(&mut self) -> Result<Vec<Operation>, LiftError> {
        let key = self.pop_stack()?;
        let map = self.pop_stack()?;
        let target = self.create_temp_variable(Type::Unknown);

        let operation = Operation::MapOp {
            operation: MapOperation::HasKey,
            map,
            key,
            value: None,
            target: Some(target.clone()),
        };

        self.push_stack(Expression::Variable(target));
        Ok(vec![operation])
    }

    /// Lift map keys operations
    fn lift_map_keys(&mut self) -> Result<Vec<Operation>, LiftError> {
        let map = self.pop_stack()?;
        let target = self.create_temp_variable(Type::Unknown);

        let operation = Operation::MapOp {
            operation: MapOperation::Keys,
            map,
            key: Expression::Literal(Literal::Null),
            value: None,
            target: Some(target.clone()),
        };

        self.push_stack(Expression::Variable(target));
        Ok(vec![operation])
    }

    /// Lift map values operations
    fn lift_map_values(&mut self) -> Result<Vec<Operation>, LiftError> {
        let map = self.pop_stack()?;
        let target = self.create_temp_variable(Type::Unknown);

        let operation = Operation::MapOp {
            operation: MapOperation::Values,
            map,
            key: Expression::Literal(Literal::Null),
            value: None,
            target: Some(target.clone()),
        };

        self.push_stack(Expression::Variable(target));
        Ok(vec![operation])
    }

    /// Lift string concatenation
    fn lift_string_concat(&mut self) -> Result<Vec<Operation>, LiftError> {
        let b = self.pop_stack()?;
        let a = self.pop_stack()?;
        let target = self.create_temp_variable(Type::Unknown);

        let operation = Operation::StringOp {
            operation: StringOperation::Concat,
            operands: vec![a, b],
            target: target.clone(),
        };

        self.push_stack(Expression::Variable(target));
        Ok(vec![operation])
    }

    /// Lift string substring
    fn lift_string_substring(&mut self) -> Result<Vec<Operation>, LiftError> {
        let length = self.pop_stack()?;
        let start = self.pop_stack()?;
        let string = self.pop_stack()?;
        let target = self.create_temp_variable(Type::Unknown);

        let operation = Operation::StringOp {
            operation: StringOperation::Substring,
            operands: vec![string, start, length],
            target: target.clone(),
        };

        self.push_stack(Expression::Variable(target));
        Ok(vec![operation])
    }

    /// Lift string left
    fn lift_string_left(&mut self) -> Result<Vec<Operation>, LiftError> {
        let count = self.pop_stack()?;
        let string = self.pop_stack()?;
        let target = self.create_temp_variable(Type::Unknown);

        let operation = Operation::StringOp {
            operation: StringOperation::Left,
            operands: vec![string, count],
            target: target.clone(),
        };

        self.push_stack(Expression::Variable(target));
        Ok(vec![operation])
    }

    /// Lift string right
    fn lift_string_right(&mut self) -> Result<Vec<Operation>, LiftError> {
        let count = self.pop_stack()?;
        let string = self.pop_stack()?;
        let target = self.create_temp_variable(Type::Unknown);

        let operation = Operation::StringOp {
            operation: StringOperation::Right,
            operands: vec![string, count],
            target: target.clone(),
        };

        self.push_stack(Expression::Variable(target));
        Ok(vec![operation])
    }

    /// Lift pack operations
    fn lift_pack(&mut self, instruction: &Instruction) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::Count(count)) = &instruction.operand {
            let mut items = Vec::new();
            for _ in 0..*count {
                items.push(self.pop_stack()?);
            }
            items.reverse();
            self.push_stack(Expression::ArrayCreate {
                element_type: Box::new(Type::Unknown),
                size: Box::new(Expression::Literal(Literal::Integer(*count as i64))),
            });
        }
        Ok(vec![])
    }

    /// Lift unpack operations
    fn lift_unpack(&mut self) -> Result<Vec<Operation>, LiftError> {
        let array = self.pop_stack()?;
        // Push array back to stack after unpacking operation
        self.push_stack(array);
        Ok(vec![])
    }

    /// Lift type check operations
    fn lift_type_check(&mut self, check_type: Type) -> Result<Vec<Operation>, LiftError> {
        let value = self.pop_stack()?;
        let target = self.create_temp_variable(Type::Unknown);

        let operation = Operation::TypeCheck {
            value,
            target_type: check_type,
            target: target.clone(),
        };

        self.push_stack(Expression::Variable(target));
        Ok(vec![operation])
    }

    /// Lift is type operations
    fn lift_is_type(&mut self, instruction: &Instruction) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::StackItemType(item_type)) = &instruction.operand {
            let check_type = self.stack_item_type_to_type(*item_type);
            self.lift_type_check(check_type)
        } else {
            Err(LiftError::InvalidOperand {
                offset: instruction.offset,
            })
        }
    }

    /// Lift convert operations
    fn lift_convert(&mut self, instruction: &Instruction) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::StackItemType(target_type)) = &instruction.operand {
            let value = self.pop_stack()?;
            let convert_type = self.stack_item_type_to_type(*target_type);
            let target = self.create_temp_variable(convert_type.clone());

            let operation = Operation::Convert {
                source: value,
                target_type: convert_type,
                target: target.clone(),
            };

            self.push_stack(Expression::Variable(target));
            Ok(vec![operation])
        } else {
            Err(LiftError::InvalidOperand {
                offset: instruction.offset,
            })
        }
    }

    /// Lift syscall operations
    fn lift_syscall(&mut self, instruction: &Instruction) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::SyscallHash(hash)) = &instruction.operand {
            let syscall_name = self.resolve_syscall_name(*hash);
            let arg_count = self.get_syscall_arg_count(*hash);

            self.validate_stack_depth(arg_count)?;

            let mut arguments = Vec::new();
            for _ in 0..arg_count {
                arguments.push(self.pop_stack()?);
            }
            arguments.reverse(); // Arguments are pushed in reverse order

            let target = if self.syscall_returns_value(*hash) {
                // Get actual return type from syscall signature
                let return_type = if let Some(signature) = self.syscall_db.get_signature(*hash) {
                    signature.return_type.clone()
                } else {
                    Type::Unknown
                };

                let var = self.create_temp_variable(return_type);
                self.push_stack(Expression::Variable(var.clone()));
                Some(var)
            } else {
                None
            };

            let operation = Operation::Syscall {
                name: syscall_name,
                arguments,
                return_type: Some(Type::Unknown),
                target,
            };

            Ok(vec![operation])
        } else {
            Err(LiftError::InvalidOperand {
                offset: instruction.offset,
            })
        }
    }

    /// Lift throw operations
    fn lift_throw(&mut self) -> Result<Vec<Operation>, LiftError> {
        let exception = self.pop_stack()?;
        Ok(vec![Operation::Throw { exception }])
    }

    /// Lift assert operations
    fn lift_assert(&mut self, has_message: bool) -> Result<Vec<Operation>, LiftError> {
        let (condition, message) = if has_message {
            let msg = self.pop_stack()?;
            let cond = self.pop_stack()?;
            (cond, Some(msg))
        } else {
            (self.pop_stack()?, None)
        };

        Ok(vec![Operation::Assert { condition, message }])
    }

    /// Lift abort operations
    fn lift_abort(&mut self, has_message: bool) -> Result<Vec<Operation>, LiftError> {
        let message = if has_message {
            Some(self.pop_stack()?)
        } else {
            None
        };

        Ok(vec![Operation::Abort { message }])
    }

    /// Lift pack struct operations  
    fn lift_pack_struct(&mut self, instruction: &Instruction) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::Count(count)) = &instruction.operand {
            let mut items = Vec::new();
            for _ in 0..*count {
                items.push(self.pop_stack()?);
            }
            items.reverse();
            self.push_stack(Expression::StructCreate {
                fields: items
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| (format!("field_{}", i), v))
                    .collect(),
            });
        }
        Ok(vec![])
    }

    /// Lift pack map operations
    fn lift_pack_map(&mut self, instruction: &Instruction) -> Result<Vec<Operation>, LiftError> {
        if let Some(Operand::Count(count)) = &instruction.operand {
            let mut entries = Vec::new();
            for _ in 0..*count {
                let value = self.pop_stack()?;
                let key = self.pop_stack()?;
                entries.push((key, value));
            }
            entries.reverse();
            self.push_stack(Expression::MapCreate { entries });
        }
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_instruction(
        opcode: OpCode,
        offset: u32,
        operand: Option<Operand>,
    ) -> Instruction {
        Instruction {
            offset,
            opcode,
            operand,
            size: 1, // Fixed size for test instruction
        }
    }

    #[test]
    fn test_lifter_creation() {
        let config = DecompilerConfig::default();
        let lifter = IRLifter::new(&config);
        assert_eq!(lifter.var_counter, 0);
        assert!(lifter.stack.is_empty());
    }

    #[test]
    fn test_find_block_boundaries() {
        let config = DecompilerConfig::default();
        let lifter = IRLifter::new(&config);

        let instructions = vec![
            create_test_instruction(OpCode::PUSHINT8, 0, Some(Operand::Integer(42))),
            create_test_instruction(OpCode::JMP, 2, Some(Operand::JumpTarget8(4))),
            create_test_instruction(OpCode::PUSHT, 6, None),
            create_test_instruction(OpCode::RET, 7, None),
        ];

        let boundaries = lifter.find_block_boundaries(&instructions);
        assert!(boundaries.contains(&0)); // Entry point
        assert!(boundaries.contains(&6)); // Jump target (2 + 4)
        assert!(boundaries.contains(&4)); // After jump instruction (2 + 2)
    }

    #[test]
    fn test_lift_simple_sequence() {
        let config = DecompilerConfig::default();
        let mut lifter = IRLifter::new(&config);

        let instructions = vec![
            create_test_instruction(OpCode::PUSHINT8, 0, Some(Operand::Integer(42))),
            create_test_instruction(OpCode::PUSHINT8, 2, Some(Operand::Integer(10))),
            create_test_instruction(OpCode::ADD, 4, None),
            create_test_instruction(OpCode::RET, 5, None),
        ];

        let result = lifter.lift_to_ir(&instructions);
        assert!(result.is_ok());

        let function = result.unwrap();
        assert_eq!(function.name, "main");
        assert!(!function.blocks.is_empty());
    }

    #[test]
    fn test_create_temp_variable() {
        let config = DecompilerConfig::default();
        let mut lifter = IRLifter::new(&config);

        let var1 = lifter.create_temp_variable(Type::Unknown);
        let var2 = lifter.create_temp_variable(Type::Unknown);

        assert_eq!(var1.name, "temp_0");
        assert_eq!(var2.name, "temp_1");
        assert_eq!(var1.id, 0);
        assert_eq!(var2.id, 1);
    }

    #[test]
    fn test_stack_underflow() {
        let config = DecompilerConfig::default();
        let mut lifter = IRLifter::new(&config);

        let result = lifter.pop_stack();
        assert!(matches!(result, Err(LiftError::StackUnderflow { .. })));
    }
}
