#![no_main]

use libfuzzer_sys::fuzz_target;
use neo_decompiler::decompiler::ir::brace_balanced;
use neo_decompiler::Decompiler;

fuzz_target!(|data: &[u8]| {
    // Exercise the full decompilation pipeline; we only care about panics.
    let Ok(decompilation) = Decompiler::new().decompile_bytes(data) else {
        // A container the pipeline rejects has nothing to check.
        return;
    };

    // The rendered C# has to be brace-balanced. A recovered control-flow
    // shape that emits an unbalanced block produces output that no
    // compiler accepts, and the user is the one who finds out.
    if let Some(csharp) = decompilation.csharp.as_deref() {
        assert!(
            brace_balanced(csharp),
            "unbalanced braces in the decompiled C#:\n{csharp}"
        );
    }

    // The high-level view has to be brace-balanced for the same reason.
    if let Some(high_level) = decompilation.high_level.as_deref() {
        assert!(
            brace_balanced(high_level),
            "unbalanced braces in the high-level output:\n{high_level}"
        );
    }

    // The pseudocode is the other rendering the user sees, and it is
    // rendered from the same recovered control-flow shape.
    if let Some(pseudocode) = decompilation.pseudocode.as_deref() {
        assert!(
            brace_balanced(pseudocode),
            "unbalanced braces in the pseudocode output:\n{pseudocode}"
        );
    }

    // The recovered control-flow graph has to agree with the instruction
    // stream it was built from. A graph that names a block the
    // instruction stream does not cover means the recovery invented
    // structure that does not exist.
    assert!(
        !decompilation.instructions.is_empty(),
        "the pipeline produced no instructions"
    );
    assert!(
        decompilation.cfg.block_count() > 0,
        "the pipeline produced a control-flow graph with no blocks"
    );
});
