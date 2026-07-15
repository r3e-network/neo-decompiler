use super::*;
use std::fs;
use std::path::PathBuf;

#[test]
fn high_level_loopif_recovers_counting_loop() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let nef = fs::read(root.join("TestingArtifacts/edgecases/LoopIf.nef")).expect("LoopIf NEF");
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef)
        .expect("decompile succeeds");
    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        !high_level.contains("loop {"),
        "counting-loop recovery must not leave loop {{:\n{high_level}"
    );
    assert!(
        high_level.contains("for (let loc0 = 0;")
            || (high_level.contains("let loc0 = 0") && high_level.contains("while ")),
        "expected counting for/while on loc0:\n{high_level}"
    );
    // Initializer must not re-execute inside the loop body.
    let header = high_level
        .find("for (")
        .or_else(|| high_level.find("while "))
        .expect("loop header");
    let body = high_level[header..]
        .find('{')
        .map(|offset| header + offset)
        .expect("loop body");
    assert!(
        !high_level[body..].contains("loc0 = 0"),
        "init must not re-run inside body:\n{high_level}"
    );
}
