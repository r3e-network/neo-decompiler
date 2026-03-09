use super::*;

#[test]
fn decompile_syscall_includes_human_name() {
    // Script: SYSCALL(System.Runtime.Platform) ; RET
    let script = [0x41, 0xB2, 0x79, 0xFC, 0xF6, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    assert!(decompilation
        .pseudocode
        .as_deref()
        .expect("pseudocode output")
        .contains("System.Runtime.Platform"));
    assert!(decompilation
        .high_level
        .as_deref()
        .expect("high-level output")
        .contains("syscall(\"System.Runtime.Platform\")"));
}

#[test]
fn void_syscall_does_not_push_stack_value() {
    // Script: PUSH0 ; PUSH0 ; SYSCALL(System.Runtime.Notify) ; RET
    // System.Runtime.Notify takes 2 args (event_name, state)
    let script = [0x10, 0x10, 0x41, 0x95, 0x01, 0x6F, 0x61, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("syscall(\"System.Runtime.Notify\""),
        "void syscall should be emitted as a statement"
    );
    assert!(
        !high_level.contains("let t0 = syscall(\"System.Runtime.Notify\")"),
        "void syscall should not push a temp onto the stack"
    );
}

#[test]
fn unknown_syscall_is_assumed_to_return_value() {
    let unknown_hash = 0xDEADBEEF;
    assert!(
        crate::syscalls::lookup(unknown_hash).is_none(),
        "fixture hash should not be present in the syscall catalog"
    );

    // Script: SYSCALL(unknown) ; RET
    let script = [0x41, 0xEF, 0xBE, 0xAD, 0xDE, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    assert!(
        decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
            .contains("let t0 = syscall(0xDEADBEEF);"),
        "unknown syscalls should conservatively push a stack value"
    );
}

#[test]
fn void_storage_syscall_is_emitted_as_statement() {
    // Script: PUSH0 ; PUSH0 ; PUSH0 ; SYSCALL(System.Storage.Put) ; RET
    // System.Storage.Put takes 3 args (context, key, value)
    let script = [0x10, 0x10, 0x10, 0x41, 0xE6, 0x3F, 0x18, 0x84, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("syscall(\"System.Storage.Put\""),
        "void storage syscall should be emitted as a statement"
    );
    assert!(
        !high_level.contains("let t0 = syscall(\"System.Storage.Put\")"),
        "void storage syscall should not push a temp onto the stack"
    );
}

#[test]
fn void_storage_local_syscall_is_emitted_as_statement() {
    // Script: PUSH0 ; PUSH0 ; SYSCALL(System.Storage.Local.Put) ; RET
    // System.Storage.Local.Put takes 2 args (key, value)
    let script = [0x10, 0x10, 0x41, 0x39, 0x0C, 0xE3, 0x0A, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("syscall(\"System.Storage.Local.Put\""),
        "void storage local syscall should be emitted as a statement"
    );
    assert!(
        !high_level.contains("let t0 = syscall(\"System.Storage.Local.Put\")"),
        "void storage local syscall should not push a temp onto the stack"
    );
}

#[test]
fn syscall_contract_call_returns_value() {
    // Script: PUSH0 x4 ; SYSCALL(System.Contract.Call) hash=0x525B7D62 ; RET
    // System.Contract.Call takes 4 args (contract_hash, method, call_flags, args)
    let script = [0x10, 0x10, 0x10, 0x10, 0x41, 0x62, 0x7D, 0x5B, 0x52, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("syscall(\"System.Contract.Call\""),
        "System.Contract.Call should resolve to human name: {high_level}"
    );
    assert!(
        high_level.contains("= syscall(\"System.Contract.Call\""),
        "System.Contract.Call returns a value and should push a temp: {high_level}"
    );
}

#[test]
fn syscall_runtime_log_is_void() {
    // Script: PUSH0 ; SYSCALL(System.Runtime.Log) hash=0x9647E7CF ; RET
    // System.Runtime.Log takes 1 arg (message)
    let script = [0x10, 0x41, 0xCF, 0xE7, 0x47, 0x96, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("syscall(\"System.Runtime.Log\""),
        "System.Runtime.Log is void and should be emitted as statement: {high_level}"
    );
    assert!(
        !high_level.contains("let t0 = syscall(\"System.Runtime.Log\")"),
        "void syscall should not push a temp: {high_level}"
    );
}

#[test]
fn syscall_runtime_log_missing_argument_emits_warning() {
    // Script: SYSCALL(System.Runtime.Log) hash=0x9647E7CF ; RET
    // No message is pushed, so the decompiler must surface a warning.
    let script = [0x41, 0xCF, 0xE7, 0x47, 0x96, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("syscall(\"System.Runtime.Log\", ???)"),
        "missing syscall argument should still be shown in output: {high_level}"
    );
    assert!(
        high_level.contains("missing syscall argument values for System.Runtime.Log"),
        "missing syscall argument should be called out inline in the output: {high_level}"
    );
    assert!(
        decompilation
            .warnings
            .iter()
            .any(|warning| warning.contains("missing syscall argument values")),
        "missing syscall argument should emit a structured warning: {:?}",
        decompilation.warnings
    );
}

#[test]
fn syscall_runtime_log_after_packed_store_reports_consumed_slot_context() {
    // Script:
    //   INITSLOT 1,0
    //   PUSHDATA1 "Hello"
    //   PUSH1
    //   PACK
    //   STLOC0
    //   SYSCALL(System.Runtime.Log)
    //   RET
    // The preceding store consumes the packed value, so the decompiler should
    // explain that the stack is empty because STLOC0 stored the last produced value.
    let script = [
        0x57, 0x01, 0x00, // INITSLOT 1,0
        0x0C, 0x05, b'H', b'e', b'l', b'l', b'o', // PUSHDATA1 "Hello"
        0x11, // PUSH1
        0xC0, // PACK
        0x70, // STLOC0
        0x41, 0xCF, 0xE7, 0x47, 0x96, // SYSCALL(System.Runtime.Log)
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
        high_level.contains("syscall(\"System.Runtime.Log\", ???)"),
        "missing syscall argument should still be rendered: {high_level}"
    );
    assert!(
        high_level.contains("preceding STLOC0 stored a packed value into loc0"),
        "packed-store context should be surfaced inline: {high_level}"
    );
    assert!(
        decompilation.warnings.iter().any(|warning| {
            warning.contains("preceding STLOC0 stored a packed value into loc0")
        }),
        "packed-store context should also be emitted as a structured warning: {:?}",
        decompilation.warnings
    );
}

#[test]
fn syscall_check_witness_returns_value() {
    // Script: PUSH0 ; SYSCALL(System.Runtime.CheckWitness) hash=0x8CEC27F8 ; RET
    // System.Runtime.CheckWitness takes 1 arg (hash_or_pubkey)
    let script = [0x10, 0x41, 0xF8, 0x27, 0xEC, 0x8C, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("syscall(\"System.Runtime.CheckWitness\""),
        "CheckWitness should resolve to human name: {high_level}"
    );
    assert!(
        high_level.contains("= syscall(\"System.Runtime.CheckWitness\""),
        "CheckWitness returns a value: {high_level}"
    );
}
