use neo_decompiler::Decompiler;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <nef_file>", args[0]);
        std::process::exit(1);
    }

    let nef_path = &args[1];
    let manifest_path = format!(
        "{}.manifest.json",
        nef_path.strip_suffix(".nef").unwrap_or(nef_path)
    );

    let decompiler = Decompiler::new();
    let mut result = decompiler.decompile_file_with_manifest(
        nef_path,
        Some(&manifest_path),
        neo_decompiler::OutputFormat::HighLevel,
    )?;

    println!("=== Decompilation Results ===\n");
    println!("Instructions: {}", result.instructions.len());
    println!("CFG Blocks: {}", result.cfg.block_count());
    println!("Call Graph Edges: {}", result.call_graph.edges.len());

    // Compute SSA
    println!("\n=== SSA Transformation ===\n");
    result.compute_ssa();

    if let Some(ssa) = result.ssa() {
        let stats = ssa.stats();
        println!("SSA Stats: {}", stats);
        println!();
        println!("{}", ssa.render());
    } else {
        println!("No SSA form available (empty CFG)");
    }

    Ok(())
}
