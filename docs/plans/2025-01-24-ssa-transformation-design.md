# SSA Transformation Design

**Date:** 2025-01-24
**Status:** Design Approved
**Priority:** Medium (README Roadmap v0.5.x)

## Overview

This document describes the design for implementing Static Single Assignment (SSA) transformation in the Neo-Decompiler. SSA ensures that each variable is assigned exactly once, making data flow analysis and optimizations significantly simpler.

## Goals

1. **Complete SSA**: Full implementation with φ nodes, dominance analysis, and variable renaming
2. **Integration**: Extend existing `Cfg` with `to_ssa()` method
3. **Internal Analysis**: φ nodes used internally for analysis, not exposed to end users
4. **Backward Compatible**: Existing IR and rendering remains unchanged

## Architecture

### Core Data Structures

```rust
// src/decompiler/cfg/ssa/variable.rs
pub struct SsaVariable {
    pub base: String,      // Original variable name (e.g., "local_0")
    pub version: usize,    // Version number for SSA uniqueness
}

pub struct PhiNode {
    pub target: SsaVariable,
    pub operands: BTreeMap<BlockId, SsaVariable>,  // predecessor -> variable
}
```

### Dominance Analysis

```rust
// src/decompiler/cfg/ssa/dominance.rs
pub struct DominanceInfo {
    pub idom: BTreeMap<BlockId, Option<BlockId>>,
    pub dominator_tree: BTreeMap<BlockId, Vec<BlockId>>,
    pub dominance_frontier: BTreeMap<BlockId, BTreeSet<BlockId>>,
}
```

**Algorithm**: Cooper-Harvey-Kennedy iterative algorithm for simplicity and adequate performance.

### SSA Form

```rust
// src/decompiler/cfg/ssa/form.rs
pub struct SsaForm {
    pub cfg: Cfg,
    pub dominance: DominanceInfo,
    pub blocks: BTreeMap<BlockId, SsaBlock>,
    pub definitions: BTreeMap<SsaVariable, BlockId>,
    pub uses: BTreeMap<SsaVariable, BTreeSet<UseSite>>,
}

pub struct SsaBlock {
    pub phi_nodes: Vec<PhiNode>,
    pub stmts: Vec<SsaStmt>,
}

pub enum SsaStmt {
    Assign { target: SsaVariable, value: SsaExpr },
    Phi(PhiNode),
    Other(Stmt),
}

pub enum SsaExpr {
    Variable(SsaVariable),
    Literal(Literal),
    Binary { op: BinOp, left: Box<SsaExpr>, right: Box<SsaExpr> },
    // ... other variants
}
```

### SSA Builder

Two-phase algorithm:

1. **φ Insertion**: Iterative worklist algorithm using dominance frontiers
2. **Variable Renaming**: Depth-first traversal of dominator tree

```rust
// src/decompiler/cfg/ssa/builder.rs
pub struct SsaBuilder {
    cfg: Cfg,
    dominance: DominanceInfo,
    versions: BTreeMap<String, usize>,
    phi_locations: BTreeMap<String, BTreeSet<BlockId>>,
}

impl SsaBuilder {
    pub fn build(mut self) -> SsaForm {
        self.insert_phi_nodes();
        self.rename_variables()
    }
}
```

### Module Structure

```
src/decompiler/cfg/ssa/
├── mod.rs              # Public exports
├── variable.rs         # SsaVariable, PhiNode
├── dominance.rs        # DominanceInfo, computation
├── form.rs             # SsaForm, SsaBlock, SsaStmt, SsaExpr
├── builder.rs          # SsaBuilder
└── render.rs           # Debug rendering (optional)
```

## Integration Points

### Cfg Extension

```rust
// src/decompiler/cfg/graph/core.rs
impl Cfg {
    pub fn to_ssa(&self) -> SsaForm {
        SsaBuilder::new(self.clone()).build()
    }

    pub fn compute_dominance(&self) -> DominanceInfo {
        dominance::compute(self)
    }
}
```

### Decompilation Update

```rust
// src/decompiler/decompilation.rs
pub struct Decompilation {
    // ... existing fields ...
    pub ssa: Option<SsaForm>,  // Lazy-computed SSA
}

impl Decompilation {
    pub fn ensure_ssa(&mut self) -> &SsaForm {
        if self.ssa.is_none() {
            self.ssa = Some(self.cfg.to_ssa());
        }
        self.ssa.as_ref().unwrap()
    }
}
```

## Implementation Tasks

### Phase 1: Core Infrastructure

1. Create `src/decompiler/cfg/ssa/` module structure
2. Implement `SsaVariable` and `PhiNode` (variable.rs)
3. Add basic tests

### Phase 2: Dominance Analysis

1. Implement `compute_immediate_dominators()`
2. Implement `build_dominator_tree()`
3. Implement `compute_dominance_frontier()`
4. Add dominance tests (diamond CFG, loops)

### Phase 3: SSA Form

1. Implement `SsaForm`, `SsaBlock`, `SsaStmt`, `SsaExpr` (form.rs)
2. Add expression/statement conversion utilities
3. Add debug rendering (render.rs)

### Phase 4: SSA Builder

1. Implement φ node insertion algorithm
2. Implement variable renaming algorithm
3. Add builder tests

### Phase 5: Integration

1. Extend `Cfg` with `to_ssa()` and `compute_dominance()`
2. Update `Decompilation` with SSA field
3. Update pipeline to support lazy SSA computation
4. Add integration tests

### Phase 6: Documentation

1. Add rustdoc to all public APIs
2. Update README with SSA capabilities
3. Add examples to `examples/`

## Testing Strategy

- **Unit Tests**: Each module has comprehensive unit tests
- **Golden Tests**: Known CFGs produce expected SSA output
- **Edge Cases**: Empty CFG, single block, complex loops
- **Existing Tests**: All 211+ existing tests must pass

## Performance Considerations

- Use `BTreeMap` for deterministic ordering
- Lazy SSA computation (only when needed)
- Iterative algorithms are O(n²) in worst case but sufficient for contract size limits (10 MiB)

## Future Enhancements

Beyond initial SSA:

- Data flow analysis (reaching definitions, live variables)
- Constant propagation using SSA
- Dead code elimination
- Register allocation hints

## References

- Cytron et al., "Efficiently Computing Static Single Assignment Form and the Control Dependence Graph"
- Cooper, Harvey, Kennedy, "A Simple, Fast Dominance Algorithm"
