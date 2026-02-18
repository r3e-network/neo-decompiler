use super::super::*;

#[test]
fn high_level_lifts_local_slots() {
    // Script: INITSLOT 1,0; PUSH1; STLOC0; LDLOC0; RET
    let script = [0x57, 0x01, 0x00, 0x11, 0x70, 0x68, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(high_level.contains("// declare 1 locals, 0 arguments"));
    assert!(high_level.contains("let loc0 = 1;"), "expected collapsed store: {high_level}");
    assert!(high_level.contains("return loc0;"));
}

#[test]
fn high_level_lifts_all_local_slot_variants() {
    // Exercise LDLOC0-6 (0x68-0x6E) and STLOC0-6 (0x70-0x76).
    // Script: INITSLOT 7,0;
    //   PUSH0; STLOC0; PUSH1; STLOC1; PUSH2; STLOC2;
    //   PUSH3; STLOC3; PUSH4; STLOC4; PUSH5; STLOC5;
    //   PUSH6; STLOC6;
    //   LDLOC0; LDLOC1; LDLOC2; LDLOC3; LDLOC4; LDLOC5; LDLOC6;
    //   RET
    let script = [
        0x57, 0x07, 0x00, // INITSLOT 7 locals, 0 args
        0x10, 0x70, // PUSH0; STLOC0
        0x11, 0x71, // PUSH1; STLOC1
        0x12, 0x72, // PUSH2; STLOC2
        0x13, 0x73, // PUSH3; STLOC3
        0x14, 0x74, // PUSH4; STLOC4
        0x15, 0x75, // PUSH5; STLOC5
        0x16, 0x76, // PUSH6; STLOC6
        0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, // LDLOC0-6
        0x40, // RET
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
        high_level.contains("// declare 7 locals, 0 arguments"),
        "should declare 7 locals: {high_level}"
    );
    for i in 0..7 {
        assert!(
            high_level.contains(&format!("loc{i}")),
            "local slot {i} should appear in output: {high_level}"
        );
    }
}

#[test]
fn high_level_lifts_all_argument_slot_variants() {
    // Exercise LDARG0-6 (0x78-0x7E) and STARG0-6 (0x80-0x86).
    // Script: INITSLOT 0,7;
    //   LDARG0; STARG0; LDARG1; STARG1; LDARG2; STARG2;
    //   LDARG3; STARG3; LDARG4; STARG4; LDARG5; STARG5;
    //   LDARG6; STARG6;
    //   RET
    let script = [
        0x57, 0x00, 0x07, // INITSLOT 0 locals, 7 args
        0x78, 0x80, // LDARG0; STARG0
        0x79, 0x81, // LDARG1; STARG1
        0x7A, 0x82, // LDARG2; STARG2
        0x7B, 0x83, // LDARG3; STARG3
        0x7C, 0x84, // LDARG4; STARG4
        0x7D, 0x85, // LDARG5; STARG5
        0x7E, 0x86, // LDARG6; STARG6
        0x40, // RET
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
        high_level.contains("// declare 0 locals, 7 arguments"),
        "should declare 7 arguments: {high_level}"
    );
    for i in 0..7 {
        assert!(
            high_level.contains(&format!("arg{i}")),
            "argument slot {i} should appear in output: {high_level}"
        );
    }
}

#[test]
fn high_level_lifts_indexed_local_slot() {
    // Exercise LDLOC (0x6F) and STLOC (0x77) with index operand.
    // Script: INITSLOT 8,0; PUSH1; STLOC 7; LDLOC 7; RET
    let script = [
        0x57, 0x08, 0x00, // INITSLOT 8 locals, 0 args
        0x11, // PUSH1
        0x77, 0x07, // STLOC 7
        0x6F, 0x07, // LDLOC 7
        0x40, // RET
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
        high_level.contains("loc7"),
        "indexed LDLOC/STLOC should reference loc7: {high_level}"
    );
}

#[test]
fn high_level_lifts_indexed_argument_slot() {
    // Exercise LDARG (0x7F) and STARG (0x87) with index operand.
    // Script: INITSLOT 0,8; LDARG 7; PUSH1; STARG 7; RET
    let script = [
        0x57, 0x00, 0x08, // INITSLOT 0 locals, 8 args
        0x7F, 0x07, // LDARG 7
        0x11, // PUSH1
        0x87, 0x07, // STARG 7
        0x40, // RET
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
        high_level.contains("arg7"),
        "indexed LDARG/STARG should reference arg7: {high_level}"
    );
}
