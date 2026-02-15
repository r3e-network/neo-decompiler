use super::*;

#[test]
fn high_level_recovers_switch_from_equality_chain() {
    // Script models:
    // loc0 = 1;
    // if (loc0 == 0) loc0 = 10;
    // else if (loc0 == 1) loc0 = 11;
    // else loc0 = 12;
    // return loc0;
    //
    // The exact branch layout is chosen so the high-level emitter produces
    // nested `if`/`else` blocks that can be rewritten into a `switch`.
    let script = [
        0x57, 0x01, 0x00, // INITSLOT 1 local, 0 args
        0x11, 0x70, // PUSH1; STLOC0
        0x68, 0x10, 0x97, // LDLOC0; PUSH0; EQUAL
        0x26, 0x06, // JMPIFNOT +6 -> else branch
        0x1A, 0x70, // PUSH10; STLOC0
        0x22, 0x0D, // JMP +13 -> end
        0x68, 0x11, 0x97, // LDLOC0; PUSH1; EQUAL
        0x26, 0x06, // JMPIFNOT +6 -> else branch
        0x1B, 0x70, // PUSH11; STLOC0
        0x22, 0x04, // JMP +4 -> end
        0x1C, 0x70, // PUSH12; STLOC0
        0x68, 0x40, // LDLOC0; RET
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
        high_level.contains("switch loc0 {"),
        "expected switch header: {high_level}"
    );
    assert!(
        high_level.contains("case 0 {"),
        "expected case 0: {high_level}"
    );
    assert!(
        high_level.contains("case 1 {"),
        "expected case 1: {high_level}"
    );
    assert!(
        high_level.contains("default {"),
        "expected default case: {high_level}"
    );
}
