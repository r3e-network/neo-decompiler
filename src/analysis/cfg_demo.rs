//! CFG Analysis Demo
//!
//! This file demonstrates the comprehensive CFG analysis capabilities
//! implemented in the neo-decompilation project.

use crate::analysis::cfg::*;
use crate::common::types::Literal;
use crate::core::ir::{Expression, IRBlock, IRFunction, Terminator};

/// Creates a demo function with complex control flow for testing CFG analysis
pub fn create_complex_demo_function() -> IRFunction {
    let mut function = IRFunction::new("complex_demo_function".to_string());

    // Block 0: Entry with conditional branch
    let mut block0 = IRBlock::new(0);
    block0.set_terminator(Terminator::Branch {
        condition: Expression::Literal(Literal::Boolean(true)),
        true_target: 1,
        false_target: 2,
    });
    block0.successors = vec![1, 2];

    // Block 1: Loop header
    let mut block1 = IRBlock::new(1);
    block1.set_terminator(Terminator::Branch {
        condition: Expression::Literal(Literal::Boolean(true)),
        true_target: 3,
        false_target: 4,
    });
    block1.predecessors = vec![0, 5]; // Entry and back edge
    block1.successors = vec![3, 4];

    // Block 2: Alternative path
    let mut block2 = IRBlock::new(2);
    block2.set_terminator(Terminator::Jump(6));
    block2.predecessors = vec![0];
    block2.successors = vec![6];

    // Block 3: Loop body
    let mut block3 = IRBlock::new(3);
    block3.set_terminator(Terminator::Branch {
        condition: Expression::Literal(Literal::Boolean(true)),
        true_target: 5,  // Continue loop
        false_target: 4, // Exit loop
    });
    block3.predecessors = vec![1];
    block3.successors = vec![5, 4];

    // Block 4: Loop exit / merge point
    let mut block4 = IRBlock::new(4);
    block4.set_terminator(Terminator::Jump(6));
    block4.predecessors = vec![1, 3];
    block4.successors = vec![6];

    // Block 5: Loop back edge
    let mut block5 = IRBlock::new(5);
    block5.set_terminator(Terminator::Jump(1)); // Back edge
    block5.predecessors = vec![3];
    block5.successors = vec![1];

    // Block 6: Final merge and return
    let mut block6 = IRBlock::new(6);
    block6.set_terminator(Terminator::Return(None));
    block6.predecessors = vec![2, 4];

    // Add all blocks
    function.add_block(block0);
    function.add_block(block1);
    function.add_block(block2);
    function.add_block(block3);
    function.add_block(block4);
    function.add_block(block5);
    function.add_block(block6);

    function.entry_block = 0;
    function.exit_blocks = vec![6];

    function
}

/// Creates a function with exception handling for testing exception flow analysis
pub fn create_exception_demo_function() -> IRFunction {
    let mut function = IRFunction::new("exception_demo_function".to_string());

    // Block 0: Entry with try-catch-finally
    let mut block0 = IRBlock::new(0);
    block0.set_terminator(Terminator::TryBlock {
        try_block: 1,
        catch_block: Some(2),
        finally_block: Some(3),
    });
    block0.successors = vec![1, 2, 3];

    // Block 1: Try block
    let mut block1 = IRBlock::new(1);
    block1.set_terminator(Terminator::Jump(3)); // Normal flow to finally
    block1.predecessors = vec![0];
    block1.successors = vec![3];

    // Block 2: Catch block
    let mut block2 = IRBlock::new(2);
    block2.set_terminator(Terminator::Jump(3)); // Exception flow to finally
    block2.predecessors = vec![0];
    block2.successors = vec![3];

    // Block 3: Finally block
    let mut block3 = IRBlock::new(3);
    block3.set_terminator(Terminator::Return(None));
    block3.predecessors = vec![0, 1, 2];

    // Add all blocks
    function.add_block(block0);
    function.add_block(block1);
    function.add_block(block2);
    function.add_block(block3);

    function.entry_block = 0;
    function.exit_blocks = vec![3];

    function
}

/// Creates a function with switch statement for testing switch edge analysis
pub fn create_switch_demo_function() -> IRFunction {
    let mut function = IRFunction::new("switch_demo_function".to_string());

    // Block 0: Entry with switch
    let mut block0 = IRBlock::new(0);
    block0.set_terminator(Terminator::Switch {
        discriminant: Expression::Literal(Literal::Integer(1)),
        targets: vec![
            (Literal::Integer(1), 1),
            (Literal::Integer(2), 2),
            (Literal::Integer(3), 3),
        ],
        default_target: Some(4),
    });
    block0.successors = vec![1, 2, 3, 4];

    // Case blocks
    for i in 1..=4 {
        let mut block = IRBlock::new(i);
        block.set_terminator(Terminator::Jump(5)); // All cases merge at block 5
        block.predecessors = vec![0];
        block.successors = vec![5];
        function.add_block(block);
    }

    // Block 5: Merge point and return
    let mut block5 = IRBlock::new(5);
    block5.set_terminator(Terminator::Return(None));
    block5.predecessors = vec![1, 2, 3, 4];

    function.add_block(block0);
    function.add_block(block5);

    function.entry_block = 0;
    function.exit_blocks = vec![5];

    function
}

/// Demonstrates CFG construction and analysis capabilities
pub fn demonstrate_cfg_analysis() -> Result<(), CFGError> {
    println!("=== Neo N3 Decompiler CFG Analysis Demonstration ===\n");

    // Demo 1: Complex control flow with loops
    println!("1. Complex Control Flow Analysis");
    println!("   Creating function with conditional branches, loops, and merge points...");

    let complex_function = create_complex_demo_function();
    let cfg_builder = CFGBuilder::new();
    let cfg = cfg_builder.build_cfg(&complex_function)?;

    println!("   ✓ CFG constructed successfully");
    println!("   - Nodes: {}", cfg.nodes.len());
    println!("   - Edges: {}", cfg.edges.len());
    println!("   - Complexity: {}", cfg.complexity.cyclomatic_complexity);
    println!("   - Loops detected: {}", cfg.loops.len());
    println!("   - SCCs: {}", cfg.sccs.len());

    if !cfg.loops.is_empty() {
        println!("   - Loop details:");
        for (i, loop_info) in cfg.loops.iter().enumerate() {
            println!(
                "     Loop {}: header={}, body_size={}, type={:?}",
                i,
                loop_info.header,
                loop_info.body.len(),
                loop_info.loop_type
            );
        }
    }

    println!("   - Reducible: {}", cfg.is_reducible());
    println!("   - Unreachable blocks: {}", cfg.unreachable_blocks.len());

    // Demo 2: Exception handling analysis
    println!("\n2. Exception Flow Analysis");
    println!("   Creating function with try-catch-finally constructs...");

    let exception_function = create_exception_demo_function();
    let exception_cfg = cfg_builder.build_cfg(&exception_function)?;

    println!("   ✓ Exception CFG constructed successfully");
    println!(
        "   - Exception regions: {}",
        exception_cfg.exception_regions.len()
    );
    println!("   - Nodes: {}", exception_cfg.nodes.len());
    println!("   - Edges: {}", exception_cfg.edges.len());

    if !exception_cfg.exception_regions.is_empty() {
        for (i, region) in exception_cfg.exception_regions.iter().enumerate() {
            println!(
                "   - Region {}: protected={}, handlers={}, finally={}",
                i,
                region.protected_blocks.len(),
                region.handler_blocks.len(),
                region.finally_blocks.len()
            );
        }
    }

    // Demo 3: Switch statement analysis
    println!("\n3. Switch Statement Analysis");
    println!("   Creating function with multi-way switch...");

    let switch_function = create_switch_demo_function();
    let switch_cfg = cfg_builder.build_cfg(&switch_function)?;

    println!("   ✓ Switch CFG constructed successfully");
    println!("   - Nodes: {}", switch_cfg.nodes.len());
    println!("   - Edges: {}", switch_cfg.edges.len());

    let switch_edges: Vec<_> = switch_cfg
        .edges
        .iter()
        .filter(|e| {
            matches!(
                e.edge_type,
                EdgeType::SwitchCase(_) | EdgeType::SwitchDefault
            )
        })
        .collect();
    println!("   - Switch edges: {}", switch_edges.len());

    // Demo 4: DOT export for visualization
    println!("\n4. CFG Visualization Export");
    println!("   Generating DOT format for visualization...");

    let dot_output = cfg.to_dot();
    println!(
        "   ✓ DOT export generated ({} characters)",
        dot_output.len()
    );
    println!("   Sample DOT output (first 200 chars):");
    println!("   {}", &dot_output[..200.min(dot_output.len())]);

    // Demo 5: Advanced analysis features
    println!("\n5. Advanced Analysis Features");
    println!("   Demonstrating additional CFG capabilities...");

    // Path analysis
    let paths_to_exit = cfg.get_paths_to(6);
    println!("   - Paths from entry to exit: {}", paths_to_exit.len());

    // Traversal demonstration
    let mut visited_order = Vec::new();
    cfg.dfs_traversal(0, |block_id| {
        visited_order.push(block_id);
    });
    println!("   - DFS traversal order: {:?}", visited_order);

    // Topological sort (will fail for cyclic CFGs)
    match cfg.topological_sort() {
        Ok(topo_order) => println!("   - Topological order: {:?}", topo_order),
        Err(CFGError::CyclicDependency) => {
            println!("   - Topological sort: Not applicable (cyclic graph)")
        }
        Err(e) => println!("   - Topological sort error: {}", e),
    }

    // Demo 6: Performance with minimal builder
    println!("\n6. Performance Comparison");
    println!("   Testing minimal vs full analysis...");

    let minimal_builder = CFGBuilder::minimal();
    let minimal_cfg = minimal_builder.build_cfg(&complex_function)?;

    println!(
        "   - Full analysis: {} complexity metrics computed",
        if cfg.complexity.cyclomatic_complexity > 0 {
            "✓"
        } else {
            "✗"
        }
    );
    println!(
        "   - Minimal analysis: {} complexity metrics computed",
        if minimal_cfg.complexity.cyclomatic_complexity > 0 {
            "✓"
        } else {
            "✗"
        }
    );

    println!("\n=== CFG Analysis Demonstration Complete ===");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complex_cfg_demo() {
        let function = create_complex_demo_function();
        let builder = CFGBuilder::new();
        let result = builder.build_cfg(&function);

        assert!(result.is_ok(), "Complex CFG construction should succeed");
        let cfg = result.unwrap();

        // Verify structure
        assert_eq!(cfg.nodes.len(), 7);
        assert_eq!(cfg.entry_block, 0);
        assert!(cfg.exit_blocks.contains(&6));

        // Verify complexity
        assert!(cfg.complexity.cyclomatic_complexity > 1);
        assert!(!cfg.loops.is_empty(), "Should detect at least one loop");
    }

    #[test]
    fn test_exception_cfg_demo() {
        let function = create_exception_demo_function();
        let builder = CFGBuilder::new();
        let result = builder.build_cfg(&function);

        assert!(result.is_ok(), "Exception CFG construction should succeed");
        let cfg = result.unwrap();

        // Verify exception handling
        assert!(
            !cfg.exception_regions.is_empty(),
            "Should detect exception regions"
        );

        // Verify edge types
        let try_edges: Vec<_> = cfg
            .edges
            .iter()
            .filter(|e| {
                matches!(
                    e.edge_type,
                    EdgeType::TryEntry | EdgeType::CatchEntry | EdgeType::FinallyEntry
                )
            })
            .collect();
        assert!(!try_edges.is_empty(), "Should have exception-related edges");
    }

    #[test]
    fn test_switch_cfg_demo() {
        let function = create_switch_demo_function();
        let builder = CFGBuilder::new();
        let result = builder.build_cfg(&function);

        assert!(result.is_ok(), "Switch CFG construction should succeed");
        let cfg = result.unwrap();

        // Verify switch edges
        let switch_edges: Vec<_> = cfg
            .edges
            .iter()
            .filter(|e| {
                matches!(
                    e.edge_type,
                    EdgeType::SwitchCase(_) | EdgeType::SwitchDefault
                )
            })
            .collect();
        assert_eq!(
            switch_edges.len(),
            4,
            "Should have 3 case edges + 1 default edge"
        );
    }

    #[test]
    fn test_cfg_analysis_demo_runs() {
        // This test ensures the demo function runs without panicking
        let result = demonstrate_cfg_analysis();
        assert!(result.is_ok(), "CFG analysis demo should run successfully");
    }

    #[test]
    fn test_dot_export() {
        let function = create_complex_demo_function();
        let builder = CFGBuilder::new();
        let cfg = builder.build_cfg(&function).unwrap();

        let dot = cfg.to_dot();
        assert!(dot.contains("digraph CFG"));
        assert!(dot.contains("->"));
        assert!(dot.len() > 100, "DOT output should be substantial");
    }

    #[test]
    fn test_reducibility_analysis() {
        let function = create_complex_demo_function();
        let builder = CFGBuilder::new();
        let cfg = builder.build_cfg(&function).unwrap();

        // Our demo function should be reducible (natural loops only)
        assert!(cfg.is_reducible(), "Demo function should be reducible");
    }
}
