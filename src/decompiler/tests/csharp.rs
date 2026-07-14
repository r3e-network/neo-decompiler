use super::*;

fn render_csharp_with_coverage(
    nef_bytes: &[u8],
    manifest: Option<ContractManifest>,
    inline_single_use_temps: bool,
    emit_trace_comments: bool,
    typed_declarations: bool,
) -> crate::decompiler::csharp::CSharpRender {
    let decompilation = Decompiler::new()
        .with_inline_single_use_temps(inline_single_use_temps)
        .with_trace_comments(emit_trace_comments)
        .with_typed_declarations(typed_declarations)
        .decompile_bytes_with_manifest(nef_bytes, manifest, OutputFormat::All)
        .expect("decompile succeeds");
    crate::decompiler::csharp::render_csharp(
        &decompilation.nef,
        &decompilation.instructions,
        decompilation.manifest.as_ref(),
        &decompilation.call_graph,
        &decompilation.method_contracts,
        &decompilation.types,
        &crate::decompiler::output_format::RenderOptions {
            inline_single_use_temps,
            emit_trace_comments,
            typed_declarations,
        },
    )
}

// Keep shared NEF fixtures and the renderer harness together; the test cases
// themselves are grouped by C# behavior in the modules below.
#[path = "csharp_body_core.rs"]
mod body_core;
#[path = "csharp_control_flow.rs"]
mod control_flow;
#[path = "csharp_fidelity.rs"]
mod fidelity;
#[path = "csharp_legacy.rs"]
mod legacy;
#[path = "csharp_literals.rs"]
mod literals;
#[path = "csharp_metadata.rs"]
mod metadata;
#[path = "csharp_methods.rs"]
mod methods;
#[path = "csharp_packing.rs"]
mod packing;
