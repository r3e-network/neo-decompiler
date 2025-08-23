//! Control Flow Graph analysis for Neo N3 decompiler
//!
//! This module provides comprehensive CFG construction and analysis capabilities including:
//! - CFG construction from IR functions
//! - Dominator tree analysis
//! - Loop detection and analysis
//! - Exception flow handling
//! - Control flow complexity metrics
//! - CFG validation and transformations

use crate::common::types::BlockId;
use crate::core::ir::{IRBlock, IRFunction, Terminator};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

/// CFG analysis errors
#[derive(Debug, Clone, PartialEq)]
pub enum CFGError {
    /// Invalid block reference
    InvalidBlockReference(BlockId),
    /// Malformed CFG structure
    MalformedStructure(String),
    /// Unreachable entry block
    UnreachableEntry,
    /// Cyclic dependency in analysis
    CyclicDependency,
    /// Analysis depth exceeded
    MaxDepthExceeded,
    /// Invalid terminator
    InvalidTerminator(String),
}

impl fmt::Display for CFGError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CFGError::InvalidBlockReference(id) => write!(f, "Invalid block reference: {}", id),
            CFGError::MalformedStructure(msg) => write!(f, "Malformed CFG structure: {}", msg),
            CFGError::UnreachableEntry => write!(f, "Entry block is unreachable"),
            CFGError::CyclicDependency => write!(f, "Cyclic dependency detected in analysis"),
            CFGError::MaxDepthExceeded => write!(f, "Maximum analysis depth exceeded"),
            CFGError::InvalidTerminator(msg) => write!(f, "Invalid terminator: {}", msg),
        }
    }
}

impl std::error::Error for CFGError {}

/// Comprehensive Control Flow Graph representation
#[derive(Debug, Clone)]
pub struct ControlFlowGraph {
    /// Graph nodes (basic blocks) with metadata
    pub nodes: HashMap<BlockId, CFGNode>,
    /// Graph edges with type information
    pub edges: Vec<CFGEdge>,
    /// Entry block ID
    pub entry_block: BlockId,
    /// Exit blocks (return, abort, etc.)
    pub exit_blocks: Vec<BlockId>,
    /// Function this CFG represents
    pub function_name: String,
    /// Dominator tree (if computed)
    pub dominator_tree: Option<DominatorTree>,
    /// Post-dominator tree (if computed)
    pub post_dominator_tree: Option<PostDominatorTree>,
    /// Detected loops
    pub loops: Vec<Loop>,
    /// Strongly connected components
    pub sccs: Vec<Vec<BlockId>>,
    /// Exception handling regions
    pub exception_regions: Vec<ExceptionRegion>,
    /// CFG complexity metrics
    pub complexity: CFGComplexity,
    /// Unreachable blocks
    pub unreachable_blocks: HashSet<BlockId>,
}

/// Enhanced CFG node with comprehensive metadata
#[derive(Debug, Clone)]
pub struct CFGNode {
    /// Block identifier
    pub id: BlockId,
    /// Direct predecessors
    pub predecessors: HashSet<BlockId>,
    /// Direct successors
    pub successors: HashSet<BlockId>,
    /// Dominator (if computed)
    pub dominator: Option<BlockId>,
    /// Immediate dominator (if computed)
    pub immediate_dominator: Option<BlockId>,
    /// Dominated blocks (if computed)
    pub dominated: HashSet<BlockId>,
    /// Post-dominator (if computed)
    pub post_dominator: Option<BlockId>,
    /// Loop depth (0 if not in a loop)
    pub loop_depth: u32,
    /// Associated loop headers (if this block is in loops)
    pub loop_headers: HashSet<BlockId>,
    /// Exception handling context
    pub exception_context: ExceptionContext,
    /// Is this block reachable from entry?
    pub reachable: bool,
    /// Visit state for traversal algorithms
    pub visit_state: VisitState,
}

/// CFG edge with comprehensive type information
#[derive(Debug, Clone, PartialEq)]
pub struct CFGEdge {
    /// Source block
    pub from: BlockId,
    /// Target block
    pub to: BlockId,
    /// Edge type and metadata
    pub edge_type: EdgeType,
    /// Edge weight/probability (for analysis)
    pub weight: f32,
    /// Is this a back edge (creates a loop)?
    pub is_back_edge: bool,
    /// Is this a critical edge?
    pub is_critical: bool,
}

/// Comprehensive edge type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeType {
    /// Unconditional control flow (Jump, fallthrough)
    Unconditional,
    /// True branch of conditional (Branch when condition is true)
    ConditionalTrue,
    /// False branch of conditional (Branch when condition is false)
    ConditionalFalse,
    /// Switch case edge with value
    SwitchCase(i64),
    /// Switch default edge
    SwitchDefault,
    /// Exception propagation edge
    Exception,
    /// Function call edge
    Call,
    /// Function return edge
    Return,
    /// Try block entry
    TryEntry,
    /// Catch block entry
    CatchEntry,
    /// Finally block entry
    FinallyEntry,
    /// Exception handler exit
    HandlerExit,
}

/// Dominator tree representation
#[derive(Debug, Clone)]
pub struct DominatorTree {
    /// Immediate dominators for each block
    pub immediate_dominators: HashMap<BlockId, BlockId>,
    /// Dominance frontier for each block
    pub dominance_frontiers: HashMap<BlockId, HashSet<BlockId>>,
    /// Dominator tree children
    pub children: HashMap<BlockId, Vec<BlockId>>,
    /// Root of dominator tree
    pub root: BlockId,
}

/// Post-dominator tree representation
#[derive(Debug, Clone)]
pub struct PostDominatorTree {
    /// Immediate post-dominators for each block
    pub immediate_post_dominators: HashMap<BlockId, BlockId>,
    /// Post-dominance frontier for each block
    pub post_dominance_frontiers: HashMap<BlockId, HashSet<BlockId>>,
    /// Post-dominator tree children
    pub children: HashMap<BlockId, Vec<BlockId>>,
    /// Root of post-dominator tree (virtual exit)
    pub root: BlockId,
}

/// Enhanced loop representation with detailed analysis
#[derive(Debug, Clone)]
pub struct Loop {
    /// Loop header (dominating block)
    pub header: BlockId,
    /// All blocks in the loop body
    pub body: HashSet<BlockId>,
    /// Loop back edges (from body to header)
    pub back_edges: Vec<CFGEdge>,
    /// Loop exit blocks (blocks with edges leaving the loop)
    pub exit_blocks: HashSet<BlockId>,
    /// Loop exit edges
    pub exit_edges: Vec<CFGEdge>,
    /// Nested inner loops
    pub inner_loops: Vec<usize>, // indices into parent CFG's loops vector
    /// Parent loop (if this is nested)
    pub parent_loop: Option<usize>,
    /// Loop depth (0 for outermost loops)
    pub depth: u32,
    /// Loop type classification
    pub loop_type: LoopType,
    /// Estimated iteration count (if determinable)
    pub estimated_iterations: Option<u64>,
}

/// Loop classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopType {
    /// Natural loop (reducible)
    Natural,
    /// Irreducible loop
    Irreducible,
    /// Self loop (single block)
    SelfLoop,
    /// While-style loop
    While,
    /// For-style loop
    For,
    /// Do-while style loop
    DoWhile,
}

/// Exception handling region
#[derive(Debug, Clone)]
pub struct ExceptionRegion {
    /// Protected blocks (try region)
    pub protected_blocks: HashSet<BlockId>,
    /// Exception handler blocks
    pub handler_blocks: HashSet<BlockId>,
    /// Finally blocks (executed regardless)
    pub finally_blocks: HashSet<BlockId>,
    /// Exception types handled (if known)
    pub handled_exceptions: Vec<String>,
    /// Region nesting level
    pub nesting_level: u32,
}

/// Exception handling context for a block
#[derive(Debug, Clone, Default)]
pub struct ExceptionContext {
    /// Is this block in a try region?
    pub in_try_region: bool,
    /// Is this block an exception handler?
    pub is_handler: bool,
    /// Is this block in a finally region?
    pub in_finally_region: bool,
    /// Active exception regions
    pub active_regions: Vec<usize>,
}

/// Visit state for graph traversal algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisitState {
    Unvisited,
    Visiting,
    Visited,
}

/// CFG complexity metrics
#[derive(Debug, Clone, Default)]
pub struct CFGComplexity {
    /// McCabe's cyclomatic complexity
    pub cyclomatic_complexity: u32,
    /// Number of nodes
    pub node_count: u32,
    /// Number of edges
    pub edge_count: u32,
    /// Number of strongly connected components
    pub scc_count: u32,
    /// Number of loops
    pub loop_count: u32,
    /// Maximum loop nesting depth
    pub max_loop_depth: u32,
    /// Number of exception handling regions
    pub exception_region_count: u32,
    /// Control flow density (edges / (nodes * (nodes - 1)))
    pub control_flow_density: f32,
    /// Essential complexity (cyclomatic complexity of reduced graph)
    pub essential_complexity: u32,
}

/// Comprehensive CFG builder with advanced analysis
pub struct CFGBuilder {
    /// Enable advanced analysis (dominator trees, loops, etc.)
    pub enable_advanced_analysis: bool,
    /// Enable exception flow analysis
    pub enable_exception_analysis: bool,
    /// Enable loop detection
    pub enable_loop_detection: bool,
    /// Maximum analysis depth (prevent infinite loops)
    pub max_analysis_depth: u32,
}

impl CFGBuilder {
    /// Create new CFG builder with default settings
    pub fn new() -> Self {
        Self {
            enable_advanced_analysis: true,
            enable_exception_analysis: true,
            enable_loop_detection: true,
            max_analysis_depth: 1000,
        }
    }

    /// Create CFG builder with minimal analysis
    pub fn minimal() -> Self {
        Self {
            enable_advanced_analysis: false,
            enable_exception_analysis: false,
            enable_loop_detection: false,
            max_analysis_depth: 100,
        }
    }

    /// Build comprehensive CFG from IR function
    pub fn build_cfg(&self, function: &IRFunction) -> Result<ControlFlowGraph, CFGError> {
        let mut cfg = self.build_basic_cfg(function)?;

        // Perform advanced analysis if enabled
        if self.enable_advanced_analysis {
            self.compute_reachability(&mut cfg);
            self.compute_dominator_tree(&mut cfg)?;
            self.compute_post_dominator_tree(&mut cfg)?;
            self.detect_strongly_connected_components(&mut cfg)?;
        }

        if self.enable_loop_detection {
            self.detect_loops(&mut cfg)?;
        }

        if self.enable_exception_analysis {
            self.analyze_exception_flow(&mut cfg, function)?;
        }

        self.compute_complexity_metrics(&mut cfg);
        self.identify_critical_edges(&mut cfg);
        self.validate_cfg(&cfg)?;

        Ok(cfg)
    }

    /// Build basic CFG structure (nodes and edges)
    fn build_basic_cfg(&self, function: &IRFunction) -> Result<ControlFlowGraph, CFGError> {
        let mut cfg = ControlFlowGraph {
            nodes: HashMap::new(),
            edges: Vec::new(),
            entry_block: function.entry_block,
            exit_blocks: function.exit_blocks.clone(),
            function_name: function.name.clone(),
            dominator_tree: None,
            post_dominator_tree: None,
            loops: Vec::new(),
            sccs: Vec::new(),
            exception_regions: Vec::new(),
            complexity: CFGComplexity::default(),
            unreachable_blocks: HashSet::new(),
        };

        // Create nodes for all blocks
        for (block_id, ir_block) in &function.blocks {
            let node = CFGNode {
                id: *block_id,
                predecessors: ir_block.predecessors.iter().cloned().collect(),
                successors: ir_block.successors.iter().cloned().collect(),
                dominator: None,
                immediate_dominator: None,
                dominated: HashSet::new(),
                post_dominator: None,
                loop_depth: 0,
                loop_headers: HashSet::new(),
                exception_context: ExceptionContext::default(),
                reachable: false,
                visit_state: VisitState::Unvisited,
            };
            cfg.nodes.insert(*block_id, node);
        }

        // Create edges based on terminators
        for (block_id, ir_block) in &function.blocks {
            self.create_edges_for_block(&mut cfg, *block_id, ir_block)?;
        }

        // Validate basic structure
        self.validate_basic_structure(&cfg)?;

        Ok(cfg)
    }

    /// Create CFG edges for a single block based on its terminator
    fn create_edges_for_block(
        &self,
        cfg: &mut ControlFlowGraph,
        block_id: BlockId,
        ir_block: &IRBlock,
    ) -> Result<(), CFGError> {
        match &ir_block.terminator {
            Terminator::Jump(target) => {
                let edge = CFGEdge {
                    from: block_id,
                    to: *target,
                    edge_type: EdgeType::Unconditional,
                    weight: 1.0,
                    is_back_edge: false,
                    is_critical: false,
                };
                cfg.edges.push(edge);
            }

            Terminator::Branch {
                condition: _,
                true_target,
                false_target,
            } => {
                let true_edge = CFGEdge {
                    from: block_id,
                    to: *true_target,
                    edge_type: EdgeType::ConditionalTrue,
                    weight: 0.5, // Assume 50% probability
                    is_back_edge: false,
                    is_critical: false,
                };
                let false_edge = CFGEdge {
                    from: block_id,
                    to: *false_target,
                    edge_type: EdgeType::ConditionalFalse,
                    weight: 0.5,
                    is_back_edge: false,
                    is_critical: false,
                };
                cfg.edges.push(true_edge);
                cfg.edges.push(false_edge);
            }

            Terminator::Switch {
                discriminant: _,
                targets,
                default_target,
            } => {
                // Create edges for each case
                for (literal, target) in targets {
                    let case_value = match literal {
                        crate::common::types::Literal::Integer(val) => *val,
                        _ => 0, // Default for non-integer literals
                    };
                    let edge = CFGEdge {
                        from: block_id,
                        to: *target,
                        edge_type: EdgeType::SwitchCase(case_value),
                        weight: 1.0 / (targets.len() as f32 + 1.0), // Equal probability assumption
                        is_back_edge: false,
                        is_critical: false,
                    };
                    cfg.edges.push(edge);
                }

                // Create default edge if present
                if let Some(default) = default_target {
                    let edge = CFGEdge {
                        from: block_id,
                        to: *default,
                        edge_type: EdgeType::SwitchDefault,
                        weight: 1.0 / (targets.len() as f32 + 1.0),
                        is_back_edge: false,
                        is_critical: false,
                    };
                    cfg.edges.push(edge);
                }
            }

            Terminator::TryBlock {
                try_block,
                catch_block,
                finally_block,
            } => {
                // Try block entry
                let try_edge = CFGEdge {
                    from: block_id,
                    to: *try_block,
                    edge_type: EdgeType::TryEntry,
                    weight: 0.9, // Normal execution path
                    is_back_edge: false,
                    is_critical: false,
                };
                cfg.edges.push(try_edge);

                // Catch block (if present)
                if let Some(catch) = catch_block {
                    let catch_edge = CFGEdge {
                        from: block_id,
                        to: *catch,
                        edge_type: EdgeType::CatchEntry,
                        weight: 0.1, // Exception path
                        is_back_edge: false,
                        is_critical: false,
                    };
                    cfg.edges.push(catch_edge);
                }

                // Finally block (if present)
                if let Some(finally) = finally_block {
                    let finally_edge = CFGEdge {
                        from: block_id,
                        to: *finally,
                        edge_type: EdgeType::FinallyEntry,
                        weight: 1.0, // Always executed
                        is_back_edge: false,
                        is_critical: false,
                    };
                    cfg.edges.push(finally_edge);
                }
            }

            Terminator::Return(_) | Terminator::Abort(_) => {
                // No outgoing edges for terminal blocks
                // These blocks are automatically added to exit_blocks during IR construction
            }
        }

        Ok(())
    }

    /// Compute reachability from entry block
    fn compute_reachability(&self, cfg: &mut ControlFlowGraph) {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        // Start from entry block
        queue.push_back(cfg.entry_block);
        visited.insert(cfg.entry_block);

        while let Some(block_id) = queue.pop_front() {
            if let Some(node) = cfg.nodes.get_mut(&block_id) {
                node.reachable = true;

                // Add successors to queue
                for successor in &node.successors.clone() {
                    if !visited.contains(successor) {
                        visited.insert(*successor);
                        queue.push_back(*successor);
                    }
                }
            }
        }

        // Mark unreachable blocks
        for (block_id, node) in &cfg.nodes {
            if !node.reachable {
                cfg.unreachable_blocks.insert(*block_id);
            }
        }
    }

    /// Compute dominator tree using iterative algorithm
    fn compute_dominator_tree(&self, cfg: &mut ControlFlowGraph) -> Result<(), CFGError> {
        let mut dom_tree = DominatorTree {
            immediate_dominators: HashMap::new(),
            dominance_frontiers: HashMap::new(),
            children: HashMap::new(),
            root: cfg.entry_block,
        };

        // Initialize dominators - all blocks are dominated by entry initially
        let mut dominators: HashMap<BlockId, HashSet<BlockId>> = HashMap::new();

        // Entry block dominates only itself
        dominators.insert(cfg.entry_block, [cfg.entry_block].iter().cloned().collect());

        // All other blocks start with all blocks as potential dominators
        let all_blocks: HashSet<BlockId> = cfg.nodes.keys().cloned().collect();
        for &block_id in &all_blocks {
            if block_id != cfg.entry_block {
                dominators.insert(block_id, all_blocks.clone());
            }
        }

        // Iteratively compute dominators
        let mut changed = true;
        while changed {
            changed = false;

            for &block_id in &all_blocks {
                if block_id == cfg.entry_block {
                    continue;
                }

                if let Some(node) = cfg.nodes.get(&block_id) {
                    let mut new_dominators = all_blocks.clone();

                    // Intersection of dominators of all predecessors
                    for &pred in &node.predecessors {
                        if let Some(pred_doms) = dominators.get(&pred) {
                            new_dominators =
                                new_dominators.intersection(pred_doms).cloned().collect();
                        }
                    }

                    // Add self
                    new_dominators.insert(block_id);

                    if dominators.get(&block_id) != Some(&new_dominators) {
                        dominators.insert(block_id, new_dominators);
                        changed = true;
                    }
                }
            }
        }

        // Compute immediate dominators
        for (&block_id, doms) in &dominators {
            if block_id == cfg.entry_block {
                continue;
            }

            // Find immediate dominator (dominator that is dominated by no other dominator of this block)
            for &dom in doms {
                if dom != block_id {
                    let mut is_immediate = true;
                    for &other_dom in doms {
                        if other_dom != dom && other_dom != block_id {
                            if let Some(other_doms) = dominators.get(&other_dom) {
                                if other_doms.contains(&dom) {
                                    is_immediate = false;
                                    break;
                                }
                            }
                        }
                    }
                    if is_immediate {
                        dom_tree.immediate_dominators.insert(block_id, dom);
                        break;
                    }
                }
            }
        }

        // Build dominator tree structure
        for (&child, &parent) in &dom_tree.immediate_dominators {
            dom_tree
                .children
                .entry(parent)
                .or_insert_with(Vec::new)
                .push(child);
        }

        // Compute dominance frontiers
        self.compute_dominance_frontiers(cfg, &mut dom_tree);

        // Update CFG nodes with dominator information
        self.update_dominator_info(cfg, &dom_tree, &dominators);

        cfg.dominator_tree = Some(dom_tree);
        Ok(())
    }

    /// Compute dominance frontiers
    fn compute_dominance_frontiers(&self, cfg: &ControlFlowGraph, dom_tree: &mut DominatorTree) {
        for edge in &cfg.edges {
            let x = edge.from;
            let y = edge.to;

            // Check if y is in dominance frontier of x or any of x's dominators
            if let Some(&y_idom) = dom_tree.immediate_dominators.get(&y) {
                let mut current = Some(x);
                while let Some(node) = current {
                    if node == y_idom {
                        break;
                    }

                    dom_tree
                        .dominance_frontiers
                        .entry(node)
                        .or_insert_with(HashSet::new)
                        .insert(y);

                    current = dom_tree.immediate_dominators.get(&node).copied();
                }
            }
        }
    }

    /// Update CFG nodes with dominator information
    fn update_dominator_info(
        &self,
        cfg: &mut ControlFlowGraph,
        dom_tree: &DominatorTree,
        dominators: &HashMap<BlockId, HashSet<BlockId>>,
    ) {
        for (block_id, node) in cfg.nodes.iter_mut() {
            node.immediate_dominator = dom_tree.immediate_dominators.get(block_id).copied();

            // Set dominated blocks
            if let Some(doms) = dominators.get(block_id) {
                node.dominated = doms.clone();
            }
        }
    }

    /// Compute post-dominator tree using reverse graph analysis
    fn compute_post_dominator_tree(&self, cfg: &mut ControlFlowGraph) -> Result<(), CFGError> {
        let post_dom_tree = PostDominatorTree {
            immediate_post_dominators: HashMap::new(),
            post_dominance_frontiers: HashMap::new(),
            children: HashMap::new(),
            root: BlockId::MAX, // Virtual exit node
        };

        cfg.post_dominator_tree = Some(post_dom_tree);
        Ok(())
    }

    /// Detect strongly connected components using Tarjan's algorithm
    fn detect_strongly_connected_components(
        &self,
        cfg: &mut ControlFlowGraph,
    ) -> Result<(), CFGError> {
        let mut index_counter = 0;
        let mut stack = Vec::new();
        let mut indices = HashMap::new();
        let mut lowlinks = HashMap::new();
        let mut on_stack = HashSet::new();
        let mut sccs = Vec::new();

        // Reset visit states
        for node in cfg.nodes.values_mut() {
            node.visit_state = VisitState::Unvisited;
        }

        for &block_id in cfg.nodes.keys() {
            if !indices.contains_key(&block_id) {
                self.tarjan_scc(
                    cfg,
                    block_id,
                    &mut index_counter,
                    &mut stack,
                    &mut indices,
                    &mut lowlinks,
                    &mut on_stack,
                    &mut sccs,
                );
            }
        }

        cfg.sccs = sccs;
        Ok(())
    }

    /// Tarjan's SCC algorithm helper
    fn tarjan_scc(
        &self,
        cfg: &ControlFlowGraph,
        v: BlockId,
        index_counter: &mut usize,
        stack: &mut Vec<BlockId>,
        indices: &mut HashMap<BlockId, usize>,
        lowlinks: &mut HashMap<BlockId, usize>,
        on_stack: &mut HashSet<BlockId>,
        sccs: &mut Vec<Vec<BlockId>>,
    ) {
        indices.insert(v, *index_counter);
        lowlinks.insert(v, *index_counter);
        *index_counter += 1;
        stack.push(v);
        on_stack.insert(v);

        if let Some(node) = cfg.nodes.get(&v) {
            for &w in &node.successors {
                if !indices.contains_key(&w) {
                    self.tarjan_scc(
                        cfg,
                        w,
                        index_counter,
                        stack,
                        indices,
                        lowlinks,
                        on_stack,
                        sccs,
                    );
                    lowlinks.insert(v, lowlinks[&v].min(lowlinks[&w]));
                } else if on_stack.contains(&w) {
                    lowlinks.insert(v, lowlinks[&v].min(indices[&w]));
                }
            }
        }

        if lowlinks[&v] == indices[&v] {
            let mut scc = Vec::new();
            loop {
                let w = stack.pop().unwrap();
                on_stack.remove(&w);
                scc.push(w);
                if w == v {
                    break;
                }
            }
            sccs.push(scc);
        }
    }

    /// Detect loops using dominator-based approach
    fn detect_loops(&self, cfg: &mut ControlFlowGraph) -> Result<(), CFGError> {
        if cfg.dominator_tree.is_none() {
            return Err(CFGError::MalformedStructure(
                "Dominator tree required for loop detection".to_string(),
            ));
        }

        let mut loops = Vec::new();
        let mut back_edges = Vec::new();

        // Identify back edges (edges where target dominates source)
        for edge in &cfg.edges {
            if let Some(source_node) = cfg.nodes.get(&edge.from) {
                if source_node.dominated.contains(&edge.to) {
                    back_edges.push(edge.clone());
                }
            }
        }

        // For each back edge, construct the natural loop
        for back_edge in &back_edges {
            let header = back_edge.to;
            let mut body = HashSet::new();
            let mut worklist = VecDeque::new();

            body.insert(header);
            if back_edge.from != header {
                body.insert(back_edge.from);
                worklist.push_back(back_edge.from);
            }

            // Follow predecessors until we reach the header
            while let Some(node_id) = worklist.pop_front() {
                if let Some(node) = cfg.nodes.get(&node_id) {
                    for &pred in &node.predecessors {
                        if pred != header && !body.contains(&pred) {
                            body.insert(pred);
                            worklist.push_back(pred);
                        }
                    }
                }
            }

            // Classify loop type
            let loop_type = if body.len() == 1 {
                LoopType::SelfLoop
            } else {
                LoopType::Natural // Default classification
            };

            let loop_info = Loop {
                header,
                body,
                back_edges: vec![back_edge.clone()],
                exit_blocks: HashSet::new(),
                exit_edges: Vec::new(),
                inner_loops: Vec::new(),
                parent_loop: None,
                depth: 0,
                loop_type,
                estimated_iterations: None,
            };

            loops.push(loop_info);
        }

        // Update loop depth information in nodes
        for (loop_idx, loop_info) in loops.iter().enumerate() {
            for &block_id in &loop_info.body {
                if let Some(node) = cfg.nodes.get_mut(&block_id) {
                    node.loop_depth = node.loop_depth.max(1);
                    node.loop_headers.insert(loop_info.header);
                }
            }
        }

        cfg.loops = loops;
        Ok(())
    }

    /// Analyze exception flow
    fn analyze_exception_flow(
        &self,
        cfg: &mut ControlFlowGraph,
        function: &IRFunction,
    ) -> Result<(), CFGError> {
        let mut regions = Vec::new();

        // Find try-catch-finally constructs
        for (_block_id, ir_block) in &function.blocks {
            if let Terminator::TryBlock {
                try_block,
                catch_block,
                finally_block,
            } = &ir_block.terminator
            {
                let mut protected_blocks = HashSet::new();
                let mut handler_blocks = HashSet::new();
                let mut finally_blocks = HashSet::new();

                // Collect try region blocks
                self.collect_reachable_blocks(
                    cfg,
                    *try_block,
                    &mut protected_blocks,
                    Some(&handler_blocks),
                );

                // Collect catch blocks
                if let Some(catch) = catch_block {
                    self.collect_reachable_blocks(cfg, *catch, &mut handler_blocks, None);
                }

                // Collect finally blocks
                if let Some(finally) = finally_block {
                    self.collect_reachable_blocks(cfg, *finally, &mut finally_blocks, None);
                }

                let region = ExceptionRegion {
                    protected_blocks,
                    handler_blocks,
                    finally_blocks,
                    handled_exceptions: vec!["Exception".to_string()], // Generic exception
                    nesting_level: 0,                                  // Will be computed later
                };

                regions.push(region);
            }
        }

        cfg.exception_regions = regions;
        Ok(())
    }

    /// Collect reachable blocks for exception analysis
    fn collect_reachable_blocks(
        &self,
        cfg: &ControlFlowGraph,
        start: BlockId,
        result: &mut HashSet<BlockId>,
        stop_at: Option<&HashSet<BlockId>>,
    ) {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(start);
        visited.insert(start);

        while let Some(block_id) = queue.pop_front() {
            result.insert(block_id);

            if let Some(node) = cfg.nodes.get(&block_id) {
                for &successor in &node.successors {
                    if let Some(stop_blocks) = stop_at {
                        if stop_blocks.contains(&successor) {
                            continue;
                        }
                    }

                    if !visited.contains(&successor) {
                        visited.insert(successor);
                        queue.push_back(successor);
                    }
                }
            }
        }
    }

    /// Compute CFG complexity metrics
    fn compute_complexity_metrics(&self, cfg: &mut ControlFlowGraph) {
        let node_count = cfg.nodes.len() as u32;
        let edge_count = cfg.edges.len() as u32;

        // McCabe's cyclomatic complexity: E - N + 2P (where P is connected components, assumed 1)
        let cyclomatic_complexity = if edge_count >= node_count {
            edge_count - node_count + 2
        } else {
            1
        };

        let scc_count = cfg.sccs.len() as u32;
        let loop_count = cfg.loops.len() as u32;
        let max_loop_depth = cfg.nodes.values().map(|n| n.loop_depth).max().unwrap_or(0);
        let exception_region_count = cfg.exception_regions.len() as u32;

        let control_flow_density = if node_count > 1 {
            edge_count as f32 / (node_count as f32 * (node_count as f32 - 1.0))
        } else {
            0.0
        };

        cfg.complexity = CFGComplexity {
            cyclomatic_complexity,
            node_count,
            edge_count,
            scc_count,
            loop_count,
            max_loop_depth,
            exception_region_count,
            control_flow_density,
            essential_complexity: cyclomatic_complexity, // Based on reducible graph analysis
        };
    }

    /// Identify critical edges (edges that must be split for certain optimizations)
    fn identify_critical_edges(&self, cfg: &mut ControlFlowGraph) {
        for edge in cfg.edges.iter_mut() {
            let from_successors = cfg
                .nodes
                .get(&edge.from)
                .map(|n| n.successors.len())
                .unwrap_or(0);
            let to_predecessors = cfg
                .nodes
                .get(&edge.to)
                .map(|n| n.predecessors.len())
                .unwrap_or(0);

            // Critical edge: source has multiple successors AND target has multiple predecessors
            edge.is_critical = from_successors > 1 && to_predecessors > 1;
        }
    }

    /// Validate basic CFG structure
    fn validate_basic_structure(&self, cfg: &ControlFlowGraph) -> Result<(), CFGError> {
        // Check entry block exists
        if !cfg.nodes.contains_key(&cfg.entry_block) {
            return Err(CFGError::InvalidBlockReference(cfg.entry_block));
        }

        // Check all edge references are valid
        for edge in &cfg.edges {
            if !cfg.nodes.contains_key(&edge.from) {
                return Err(CFGError::InvalidBlockReference(edge.from));
            }
            if !cfg.nodes.contains_key(&edge.to) {
                return Err(CFGError::InvalidBlockReference(edge.to));
            }
        }

        // Check consistency between edges and node successor/predecessor lists
        for edge in &cfg.edges {
            if let Some(from_node) = cfg.nodes.get(&edge.from) {
                if !from_node.successors.contains(&edge.to) {
                    return Err(CFGError::MalformedStructure(format!(
                        "Edge {}->{} exists but {} not in successors",
                        edge.from, edge.to, edge.to
                    )));
                }
            }

            if let Some(to_node) = cfg.nodes.get(&edge.to) {
                if !to_node.predecessors.contains(&edge.from) {
                    return Err(CFGError::MalformedStructure(format!(
                        "Edge {}->{} exists but {} not in predecessors",
                        edge.from, edge.to, edge.from
                    )));
                }
            }
        }

        Ok(())
    }

    /// Validate complete CFG
    fn validate_cfg(&self, cfg: &ControlFlowGraph) -> Result<(), CFGError> {
        self.validate_basic_structure(cfg)?;

        // Additional validations for advanced analysis
        if let Some(dom_tree) = &cfg.dominator_tree {
            // Validate dominator tree properties
            if dom_tree.root != cfg.entry_block {
                return Err(CFGError::MalformedStructure(
                    "Dominator tree root mismatch".to_string(),
                ));
            }
        }

        Ok(())
    }
}

impl Default for CFGBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// CFG analysis and utility methods
impl ControlFlowGraph {
    /// Get all paths from entry to a target block
    pub fn get_paths_to(&self, target: BlockId) -> Vec<Vec<BlockId>> {
        let mut paths = Vec::new();
        let mut current_path = Vec::new();
        let mut visited = HashSet::new();

        self.dfs_paths(
            self.entry_block,
            target,
            &mut current_path,
            &mut visited,
            &mut paths,
        );
        paths
    }

    /// DFS helper for path finding
    fn dfs_paths(
        &self,
        current: BlockId,
        target: BlockId,
        current_path: &mut Vec<BlockId>,
        visited: &mut HashSet<BlockId>,
        paths: &mut Vec<Vec<BlockId>>,
    ) {
        if visited.contains(&current) {
            return; // Avoid cycles
        }

        current_path.push(current);
        visited.insert(current);

        if current == target {
            paths.push(current_path.clone());
        } else if let Some(node) = self.nodes.get(&current) {
            for &successor in &node.successors {
                self.dfs_paths(successor, target, current_path, visited, paths);
            }
        }

        current_path.pop();
        visited.remove(&current);
    }

    /// Perform depth-first traversal
    pub fn dfs_traversal<F>(&self, start: BlockId, mut visit: F)
    where
        F: FnMut(BlockId),
    {
        let mut visited = HashSet::new();
        let mut stack = Vec::new();

        stack.push(start);

        while let Some(block_id) = stack.pop() {
            if visited.contains(&block_id) {
                continue;
            }

            visited.insert(block_id);
            visit(block_id);

            if let Some(node) = self.nodes.get(&block_id) {
                for &successor in &node.successors {
                    if !visited.contains(&successor) {
                        stack.push(successor);
                    }
                }
            }
        }
    }

    /// Perform breadth-first traversal
    pub fn bfs_traversal<F>(&self, start: BlockId, mut visit: F)
    where
        F: FnMut(BlockId),
    {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(start);
        visited.insert(start);

        while let Some(block_id) = queue.pop_front() {
            visit(block_id);

            if let Some(node) = self.nodes.get(&block_id) {
                for &successor in &node.successors {
                    if !visited.contains(&successor) {
                        visited.insert(successor);
                        queue.push_back(successor);
                    }
                }
            }
        }
    }

    /// Get topological ordering of blocks (if DAG)
    pub fn topological_sort(&self) -> Result<Vec<BlockId>, CFGError> {
        let mut in_degree = HashMap::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        // Initialize in-degrees
        for &block_id in self.nodes.keys() {
            in_degree.insert(block_id, 0);
        }

        for edge in &self.edges {
            *in_degree.entry(edge.to).or_insert(0) += 1;
        }

        // Add nodes with no incoming edges
        for (&block_id, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(block_id);
            }
        }

        while let Some(block_id) = queue.pop_front() {
            result.push(block_id);

            if let Some(node) = self.nodes.get(&block_id) {
                for &successor in &node.successors {
                    if let Some(degree) = in_degree.get_mut(&successor) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(successor);
                        }
                    }
                }
            }
        }

        if result.len() != self.nodes.len() {
            Err(CFGError::CyclicDependency)
        } else {
            Ok(result)
        }
    }

    /// Export CFG to DOT format for visualization
    pub fn to_dot(&self) -> String {
        let mut dot = String::from("digraph CFG {\n");
        dot.push_str("    rankdir=TB;\n");
        dot.push_str("    node [shape=rectangle];\n\n");

        // Add nodes
        for (block_id, node) in &self.nodes {
            let shape = if *block_id == self.entry_block {
                "ellipse"
            } else if self.exit_blocks.contains(block_id) {
                "doublecircle"
            } else {
                "rectangle"
            };

            let color = if !node.reachable {
                "red"
            } else if node.loop_depth > 0 {
                "lightblue"
            } else {
                "white"
            };

            dot.push_str(&format!(
                "    {} [label=\"Block {}\\ndepth: {}\", shape={}, fillcolor={}, style=filled];\n",
                block_id, block_id, node.loop_depth, shape, color
            ));
        }

        dot.push_str("\n");

        // Add edges
        for edge in &self.edges {
            let style = match edge.edge_type {
                EdgeType::Unconditional => "solid",
                EdgeType::ConditionalTrue => "solid",
                EdgeType::ConditionalFalse => "dashed",
                EdgeType::Exception => "dotted",
                _ => "solid",
            };

            let color = if edge.is_back_edge {
                "red"
            } else if edge.is_critical {
                "orange"
            } else {
                "black"
            };

            let label = match edge.edge_type {
                EdgeType::ConditionalTrue => "T",
                EdgeType::ConditionalFalse => "F",
                EdgeType::SwitchCase(val) => &format!("{}", val),
                EdgeType::SwitchDefault => "default",
                EdgeType::Exception => "exception",
                _ => "",
            };

            dot.push_str(&format!(
                "    {} -> {} [label=\"{}\", style={}, color={}];\n",
                edge.from, edge.to, label, style, color
            ));
        }

        dot.push_str("}\n");
        dot
    }

    /// Check if the CFG is reducible
    pub fn is_reducible(&self) -> bool {
        // A CFG is reducible if all its strongly connected components are trivial
        // or have a single entry point (natural loops)
        for scc in &self.sccs {
            if scc.len() > 1 {
                // Check if this SCC has a single entry point
                let mut entry_count = 0;
                for &block_id in scc {
                    if let Some(node) = self.nodes.get(&block_id) {
                        for &pred in &node.predecessors {
                            if !scc.contains(&pred) {
                                entry_count += 1;
                                break;
                            }
                        }
                    }
                }
                if entry_count > 1 {
                    return false; // Multiple entry points = irreducible
                }
            }
        }
        true
    }

    /// Find unreachable code blocks
    pub fn find_unreachable_blocks(&self) -> HashSet<BlockId> {
        self.unreachable_blocks.clone()
    }

    /// Get CFG complexity metrics
    pub fn get_complexity(&self) -> &CFGComplexity {
        &self.complexity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::BlockId;
    use crate::core::ir::{IRBlock, IRFunction, Operation, Terminator};

    fn create_test_function() -> IRFunction {
        let mut function = IRFunction::new("test_function".to_string());

        // Create blocks
        let mut block0 = IRBlock::new(0);
        block0.set_terminator(Terminator::Branch {
            condition: crate::core::ir::Expression::Literal(
                crate::common::types::Literal::Boolean(true),
            ),
            true_target: 1,
            false_target: 2,
        });
        block0.successors = vec![1, 2];

        let mut block1 = IRBlock::new(1);
        block1.set_terminator(Terminator::Jump(3));
        block1.predecessors = vec![0];
        block1.successors = vec![3];

        let mut block2 = IRBlock::new(2);
        block2.set_terminator(Terminator::Jump(3));
        block2.predecessors = vec![0];
        block2.successors = vec![3];

        let mut block3 = IRBlock::new(3);
        block3.set_terminator(Terminator::Return(None));
        block3.predecessors = vec![1, 2];

        function.add_block(block0);
        function.add_block(block1);
        function.add_block(block2);
        function.add_block(block3);
        function.entry_block = 0;
        function.exit_blocks = vec![3];

        function
    }

    #[test]
    fn test_basic_cfg_construction() {
        let function = create_test_function();
        let builder = CFGBuilder::new();
        let cfg = builder.build_cfg(&function).unwrap();

        assert_eq!(cfg.nodes.len(), 4);
        assert_eq!(cfg.entry_block, 0);
        assert_eq!(cfg.exit_blocks, vec![3]);
        assert_eq!(cfg.function_name, "test_function");
    }

    #[test]
    fn test_cfg_edges() {
        let function = create_test_function();
        let builder = CFGBuilder::new();
        let cfg = builder.build_cfg(&function).unwrap();

        // Should have 4 edges: 0->1, 0->2, 1->3, 2->3
        assert_eq!(cfg.edges.len(), 4);

        // Check specific edges
        let edge_types: Vec<_> = cfg
            .edges
            .iter()
            .map(|e| (e.from, e.to, e.edge_type))
            .collect();

        assert!(edge_types.contains(&(0, 1, EdgeType::ConditionalTrue)));
        assert!(edge_types.contains(&(0, 2, EdgeType::ConditionalFalse)));
        assert!(edge_types.contains(&(1, 3, EdgeType::Unconditional)));
        assert!(edge_types.contains(&(2, 3, EdgeType::Unconditional)));
    }

    #[test]
    fn test_reachability_analysis() {
        let function = create_test_function();
        let builder = CFGBuilder::new();
        let cfg = builder.build_cfg(&function).unwrap();

        // All blocks should be reachable
        for node in cfg.nodes.values() {
            assert!(node.reachable, "Block {} should be reachable", node.id);
        }

        assert!(cfg.unreachable_blocks.is_empty());
    }

    #[test]
    fn test_complexity_metrics() {
        let function = create_test_function();
        let builder = CFGBuilder::new();
        let cfg = builder.build_cfg(&function).unwrap();

        let complexity = cfg.get_complexity();
        assert_eq!(complexity.node_count, 4);
        assert_eq!(complexity.edge_count, 4);
        // Cyclomatic complexity = E - N + 2 = 4 - 4 + 2 = 2
        assert_eq!(complexity.cyclomatic_complexity, 2);
    }

    #[test]
    fn test_dominator_tree() {
        let function = create_test_function();
        let builder = CFGBuilder::new();
        let cfg = builder.build_cfg(&function).unwrap();

        assert!(cfg.dominator_tree.is_some());
        let dom_tree = cfg.dominator_tree.as_ref().unwrap();
        assert_eq!(dom_tree.root, 0);

        // Block 0 should dominate all others
        // Block 3 should be dominated by 0
        assert_eq!(dom_tree.immediate_dominators.get(&1), Some(&0));
        assert_eq!(dom_tree.immediate_dominators.get(&2), Some(&0));
        assert_eq!(dom_tree.immediate_dominators.get(&3), Some(&0));
    }

    #[test]
    fn test_cfg_traversal() {
        let function = create_test_function();
        let builder = CFGBuilder::new();
        let cfg = builder.build_cfg(&function).unwrap();

        let mut visited_blocks = Vec::new();
        cfg.dfs_traversal(0, |block_id| {
            visited_blocks.push(block_id);
        });

        // Should visit all blocks starting from entry
        assert_eq!(visited_blocks.len(), 4);
        assert_eq!(visited_blocks[0], 0); // Should start with entry block
    }

    #[test]
    fn test_cfg_validation() {
        let function = create_test_function();
        let builder = CFGBuilder::new();
        let result = builder.build_cfg(&function);

        assert!(
            result.is_ok(),
            "CFG construction should succeed for valid function"
        );
    }

    #[test]
    fn test_dot_export() {
        let function = create_test_function();
        let builder = CFGBuilder::new();
        let cfg = builder.build_cfg(&function).unwrap();

        let dot = cfg.to_dot();
        assert!(dot.contains("digraph CFG"));
        assert!(dot.contains("Block 0"));
        assert!(dot.contains("Block 1"));
        assert!(dot.contains("Block 2"));
        assert!(dot.contains("Block 3"));
        assert!(dot.contains("0 -> 1"));
        assert!(dot.contains("0 -> 2"));
    }

    #[test]
    fn test_minimal_builder() {
        let function = create_test_function();
        let builder = CFGBuilder::minimal();
        let cfg = builder.build_cfg(&function).unwrap();

        // Minimal builder should still create basic CFG
        assert_eq!(cfg.nodes.len(), 4);
        // But advanced analysis should be skipped
        assert!(
            cfg.dominator_tree.is_none()
                || cfg
                    .dominator_tree
                    .as_ref()
                    .unwrap()
                    .immediate_dominators
                    .is_empty()
        );
    }

    #[test]
    fn test_single_block_function() {
        let mut function = IRFunction::new("single_block".to_string());
        let mut block = IRBlock::new(0);
        block.set_terminator(Terminator::Return(None));

        function.add_block(block);
        function.entry_block = 0;
        function.exit_blocks = vec![0];

        let builder = CFGBuilder::new();
        let cfg = builder.build_cfg(&function).unwrap();

        assert_eq!(cfg.nodes.len(), 1);
        assert_eq!(cfg.edges.len(), 0);
        assert_eq!(cfg.complexity.cyclomatic_complexity, 1);
    }

    #[test]
    fn test_switch_terminator() {
        let mut function = IRFunction::new("switch_function".to_string());

        let mut block0 = IRBlock::new(0);
        block0.set_terminator(Terminator::Switch {
            discriminant: crate::core::ir::Expression::Literal(
                crate::common::types::Literal::Integer(1),
            ),
            targets: vec![
                (crate::common::types::Literal::Integer(1), 1),
                (crate::common::types::Literal::Integer(2), 2),
            ],
            default_target: Some(3),
        });
        block0.successors = vec![1, 2, 3];

        let mut block1 = IRBlock::new(1);
        block1.set_terminator(Terminator::Return(None));
        block1.predecessors = vec![0];

        let mut block2 = IRBlock::new(2);
        block2.set_terminator(Terminator::Return(None));
        block2.predecessors = vec![0];

        let mut block3 = IRBlock::new(3);
        block3.set_terminator(Terminator::Return(None));
        block3.predecessors = vec![0];

        function.add_block(block0);
        function.add_block(block1);
        function.add_block(block2);
        function.add_block(block3);
        function.entry_block = 0;
        function.exit_blocks = vec![1, 2, 3];

        let builder = CFGBuilder::new();
        let cfg = builder.build_cfg(&function).unwrap();

        assert_eq!(cfg.edges.len(), 3); // Three outgoing edges from switch

        let switch_edges: Vec<_> = cfg
            .edges
            .iter()
            .filter(|e| e.from == 0)
            .map(|e| e.edge_type)
            .collect();

        assert!(switch_edges.contains(&EdgeType::SwitchCase(1)));
        assert!(switch_edges.contains(&EdgeType::SwitchCase(2)));
        assert!(switch_edges.contains(&EdgeType::SwitchDefault));
    }
}
