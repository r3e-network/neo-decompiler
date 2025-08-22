# Neo N3 Decompiler - Control Flow Graph (CFG) Implementation

## Overview

This document describes the comprehensive Control Flow Graph (CFG) analysis framework implemented for the Neo N3 decompiler. The implementation provides advanced CFG construction and analysis capabilities specifically designed for Neo N3 smart contract bytecode analysis and decompilation.

## Features

### Core CFG Data Structures

#### 1. **ControlFlowGraph**
- **Graph nodes**: Basic blocks with comprehensive metadata
- **Graph edges**: Typed edges with flow analysis information
- **Entry/Exit tracking**: Clear entry point and multiple exit point support
- **Advanced analysis results**: Dominator trees, loops, SCCs, exception regions
- **Complexity metrics**: McCabe's cyclomatic complexity and other quality metrics
- **Unreachable code detection**: Identifies dead code blocks

#### 2. **CFGNode** (Enhanced Basic Block)
- **Predecessor/Successor tracking**: Complete connectivity information
- **Dominator information**: Immediate dominators and dominated blocks
- **Loop context**: Loop depth and associated loop headers
- **Exception context**: Try/catch/finally region membership
- **Reachability**: Dead code identification
- **Visit state**: Support for graph traversal algorithms

#### 3. **CFGEdge** (Comprehensive Edge Types)
- **Unconditional flow**: Jump and fallthrough edges
- **Conditional flow**: True/false branches with probability weighting
- **Switch edges**: Multi-way branches with case values
- **Exception edges**: Try/catch/finally flow
- **Back edges**: Loop identification support
- **Critical edges**: Optimization-relevant edge classification

### Advanced Analysis Algorithms

#### 1. **Dominator Tree Construction**
- **Algorithm**: Simplified iterative dominator computation
- **Features**: 
  - Immediate dominator calculation
  - Dominance frontier computation
  - Dominator tree structure building
- **Applications**: 
  - SSA form construction preparation
  - Control dependence analysis
  - Loop detection foundation

#### 2. **Loop Detection and Analysis**
- **Method**: Dominator-based natural loop identification
- **Features**:
  - Back edge identification
  - Natural loop body construction
  - Loop nesting level calculation
  - Loop type classification (Natural, SelfLoop, Irreducible)
- **Classifications**:
  - Natural loops (reducible)
  - Self loops (single block)
  - Irreducible loops (complex control flow)

#### 3. **Strongly Connected Components (SCCs)**
- **Algorithm**: Tarjan's SCC algorithm
- **Features**:
  - Efficient cycle detection
  - Reducibility analysis foundation
  - Complex control flow identification
- **Applications**:
  - Irreducible loop detection
  - Control flow complexity assessment

#### 4. **Exception Flow Analysis**
- **Neo N3 specific**: Handles TryBlock terminators
- **Features**:
  - Protected region identification
  - Exception handler mapping
  - Finally block tracking
  - Exception propagation path analysis
- **Support**: Try-catch-finally constructs native to Neo N3 VM

### CFG Construction Algorithm

#### 1. **Basic CFG Building**
```
1. Create nodes for all IR basic blocks
2. Extract predecessor/successor relationships
3. Process terminators to create typed edges:
   - Jump → Unconditional edges
   - Branch → ConditionalTrue/False edges
   - Switch → SwitchCase/Default edges
   - TryBlock → TryEntry/CatchEntry/FinallyEntry edges
   - Return/Abort → Terminal blocks
4. Validate basic structure integrity
```

#### 2. **Advanced Analysis Pipeline**
```
1. Compute reachability from entry block
2. Build dominator tree structure
3. Detect strongly connected components
4. Identify natural loops and back edges
5. Analyze exception handling regions
6. Calculate complexity metrics
7. Identify critical edges
8. Validate complete CFG structure
```

### Exception Flow Handling

#### Neo N3 TryBlock Support
- **TryBlock terminator**: Native Neo N3 exception construct
- **Three-way branching**: Try, catch, finally paths
- **Exception propagation**: Proper exception edge modeling
- **Region analysis**: Protected blocks and handler identification

#### Exception Context Tracking
- **Try regions**: Blocks protected by exception handlers
- **Handler blocks**: Exception handling code regions
- **Finally blocks**: Cleanup code that always executes
- **Nesting levels**: Proper nested exception handling

### CFG Validation and Metrics

#### 1. **Structural Validation**
- **Block reference validation**: All edge targets exist
- **Consistency checking**: Edge lists match node connectivity
- **Entry block verification**: Entry block exists and is valid
- **Dominator tree validation**: Tree properties maintained

#### 2. **Complexity Metrics**
- **McCabe's Cyclomatic Complexity**: E - N + 2P formula
- **Node and edge counts**: Basic graph statistics
- **Loop metrics**: Loop count and maximum nesting depth
- **Control flow density**: Edge density calculation
- **Essential complexity**: Reduced graph complexity

#### 3. **Quality Analysis**
- **Reducibility checking**: Natural vs. irreducible control flow
- **Unreachable code detection**: Dead code identification
- **Critical edge identification**: Optimization-relevant edges

### Utility Methods and Analysis

#### 1. **Graph Traversal**
- **Depth-First Search (DFS)**: Complete graph traversal
- **Breadth-First Search (BFS)**: Level-order traversal
- **Path finding**: All paths from entry to target
- **Topological sorting**: DAG ordering (when applicable)

#### 2. **Visualization Support**
- **DOT format export**: Graphviz-compatible visualization
- **Node styling**: Entry/exit/loop block differentiation
- **Edge styling**: Edge type and criticality visualization
- **Color coding**: Reachability and loop depth indication

### Performance and Scalability

#### 1. **Builder Configuration**
- **Full analysis mode**: All advanced features enabled
- **Minimal mode**: Basic CFG construction only
- **Selective analysis**: Configurable analysis components
- **Performance tuning**: Maximum depth limits

#### 2. **Complexity Considerations**
- **Dominator computation**: O(n²) simplified algorithm
- **SCC detection**: O(n + e) Tarjan's algorithm  
- **Loop detection**: O(n + e) dominator-based approach
- **Memory usage**: Comprehensive metadata storage

## Usage Examples

### Basic CFG Construction
```rust
use crate::analysis::cfg::CFGBuilder;

let builder = CFGBuilder::new();
let cfg = builder.build_cfg(&ir_function)?;

println!("Cyclomatic complexity: {}", cfg.complexity.cyclomatic_complexity);
println!("Loops detected: {}", cfg.loops.len());
```

### Minimal Analysis Mode
```rust
let builder = CFGBuilder::minimal();
let cfg = builder.build_cfg(&ir_function)?;
// Fast construction with basic analysis only
```

### Advanced Analysis Access
```rust
// Access dominator information
if let Some(dom_tree) = &cfg.dominator_tree {
    for (block, idom) in &dom_tree.immediate_dominators {
        println!("Block {} dominated by {}", block, idom);
    }
}

// Analyze loops
for (i, loop_info) in cfg.loops.iter().enumerate() {
    println!("Loop {}: header={}, size={}, type={:?}", 
             i, loop_info.header, loop_info.body.len(), loop_info.loop_type);
}
```

### CFG Visualization
```rust
let dot_output = cfg.to_dot();
std::fs::write("cfg.dot", dot_output)?;
// Generate with: dot -Tpng cfg.dot -o cfg.png
```

## Integration with Neo N3 Decompiler

### 1. **IR Integration**
- **IRFunction input**: Works with decompiler's IR representation
- **Terminator handling**: Supports all Neo N3 VM control flow constructs
- **Block structure**: Leverages existing basic block organization

### 2. **Analysis Pipeline**
- **Pre-optimization**: CFG analysis before optimization passes
- **Type inference preparation**: Dominator trees for SSA construction
- **Code quality assessment**: Complexity metrics for analysis reporting

### 3. **Smart Contract Specifics**
- **Exception handling**: Neo N3 TryBlock construct support
- **Call flow**: CALL/CALLA instruction handling
- **Switch statements**: Multi-way branch analysis

## Error Handling

### CFG Analysis Errors
- **InvalidBlockReference**: Referenced block doesn't exist
- **MalformedStructure**: Inconsistent CFG structure
- **UnreachableEntry**: Entry block not reachable
- **CyclicDependency**: Unexpected cycles in analysis
- **MaxDepthExceeded**: Analysis depth limits reached

### Validation and Recovery
- **Structural validation**: Comprehensive integrity checking
- **Graceful degradation**: Minimal analysis on complex cases
- **Error reporting**: Detailed error context and suggestions

## Testing and Validation

### Test Coverage
- **Basic CFG construction**: Simple control flow patterns
- **Complex control flow**: Nested loops and conditionals
- **Exception handling**: Try-catch-finally constructs
- **Switch statements**: Multi-way branch patterns
- **Edge cases**: Single blocks, unreachable code
- **Performance testing**: Large function analysis

### Validation Methods
- **Dominator property verification**: Tree structure validation
- **Loop correctness**: Natural loop property checking
- **Reachability consistency**: Dead code detection accuracy
- **Complexity metric accuracy**: Cyclomatic complexity verification

## Future Enhancements

### Planned Features
1. **Advanced Loop Analysis**: Loop invariant identification, induction variable detection
2. **Control Dependence**: Control dependence graph construction
3. **Profile-Guided Analysis**: Execution frequency integration
4. **Optimization Integration**: CFG transformation for optimization passes
5. **Incremental Analysis**: Efficient CFG updates for code changes

### Performance Improvements
1. **Lengauer-Tarjan Algorithm**: Faster dominator computation
2. **Incremental SCC**: Efficient SCC updates
3. **Memory Optimization**: Reduced metadata overhead
4. **Parallel Analysis**: Multi-threaded analysis for large functions

## Conclusion

The Neo N3 decompiler CFG implementation provides a comprehensive foundation for advanced control flow analysis. With support for all Neo N3 VM control constructs, advanced analysis algorithms, and extensive validation, it enables sophisticated decompilation and optimization capabilities while maintaining excellent performance and reliability.

The modular design allows for both lightweight basic analysis and comprehensive advanced analysis depending on requirements, making it suitable for various use cases from basic decompilation to advanced static analysis and optimization.