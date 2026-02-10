use super::*;

#[test]
fn high_level_lifts_try_finally_blocks() {
    // Script models:
    // TRY (finally at +3)
    //   PUSH1
    //   ENDTRY +2 (resume after finally)
    // FINALLY:
    //   PUSH2
    //   ENDFINALLY
    // RET
    let script = [0x3B, 0x00, 0x03, 0x11, 0x3D, 0x02, 0x12, 0x3F, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("try {"),
        "missing try block: {high_level}"
    );
    assert!(
        high_level.contains("finally {"),
        "missing finally block: {high_level}"
    );
    assert!(
        !high_level.contains("end-try ->"),
        "structured try should suppress ENDTRY note: {high_level}"
    );
    assert!(
        !high_level.contains("endfinally"),
        "structured try should suppress ENDFINALLY note: {high_level}"
    );
}

#[test]
fn high_level_lifts_try_catch_blocks() {
    // Script models:
    // TRY (catch at +3)
    //   PUSH1
    //   ENDTRY +3 (skip catch)
    // CATCH:
    //   PUSH2
    //   ENDTRY +0
    // RET
    let script = [0x3B, 0x03, 0x00, 0x11, 0x3D, 0x03, 0x12, 0x3D, 0x00, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("try {"),
        "missing try header: {high_level}"
    );
    assert!(
        high_level.contains("catch {"),
        "missing catch header: {high_level}"
    );
    assert!(
        !high_level.contains("end-try ->"),
        "structured try should suppress ENDTRY notes: {high_level}"
    );
}

#[test]
fn high_level_lifts_try_catch_finally_blocks() {
    // Script models:
    // TRY (catch at +3, finally at +6)
    //   PUSH1
    //   ENDTRY +5 (skip catch/finally)
    // CATCH:
    //   PUSH2
    //   ENDTRY +2 (skip finally)
    // FINALLY:
    //   PUSH3
    //   ENDFINALLY
    // RET
    let script = [
        0x3B, 0x03, 0x06, 0x11, 0x3D, 0x05, 0x12, 0x3D, 0x02, 0x13, 0x3F, 0x40,
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("try {"),
        "missing try header: {high_level}"
    );
    assert!(
        high_level.contains("catch {"),
        "missing catch header: {high_level}"
    );
    assert!(
        high_level.contains("finally {"),
        "missing finally header: {high_level}"
    );
    assert!(
        !high_level.contains("endfinally"),
        "structured try should suppress ENDFINALLY note: {high_level}"
    );
}

#[test]
fn high_level_lifts_try_finally_with_throw_inside() {
    // TRY (finally at +4)
    //   PUSH1
    //   THROW
    //   ENDTRY +2 (resume after finally)
    // FINALLY:
    //   PUSH2
    //   ENDFINALLY
    // RET
    let script = [
        0x3B, 0x00, 0x04, // TRY (catch=0, finally=+4)
        0x11,             // PUSH1
        0x3A,             // THROW
        0x3D, 0x02,       // ENDTRY +2
        0x12,             // PUSH2
        0x3F,             // ENDFINALLY
        0x40,             // RET
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("try {"),
        "missing try block: {high_level}"
    );
    assert!(
        high_level.contains("throw("),
        "THROW inside try should be lifted: {high_level}"
    );
    assert!(
        high_level.contains("finally {"),
        "missing finally block: {high_level}"
    );
}

#[test]
fn high_level_lifts_try_catch_with_abort_in_catch() {
    // TRY (catch at +3)
    //   PUSH1
    //   ENDTRY +3 (skip catch)
    // CATCH:
    //   ABORT
    //   ENDTRY +0
    // RET
    let script = [
        0x3B, 0x03, 0x00, // TRY (catch=+3, finally=0)
        0x11,             // PUSH1
        0x3D, 0x03,       // ENDTRY +3
        0x38,             // ABORT
        0x3D, 0x00,       // ENDTRY +0
        0x40,             // RET
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("try {"),
        "missing try header: {high_level}"
    );
    assert!(
        high_level.contains("catch {"),
        "missing catch header: {high_level}"
    );
    assert!(
        high_level.contains("abort()"),
        "ABORT inside catch should be lifted: {high_level}"
    );
}
