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
    // Script: SYSCALL(System.Runtime.Notify) ; RET
    let script = [0x41, 0x95, 0x01, 0x6F, 0x61, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    assert!(
        decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
            .contains("syscall(\"System.Runtime.Notify\");"),
        "void syscall should be emitted as a statement"
    );
    assert!(
        !decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
            .contains("let t0 = syscall(\"System.Runtime.Notify\")"),
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
    // Script: SYSCALL(System.Storage.Put) ; RET
    let script = [0x41, 0xE6, 0x3F, 0x18, 0x84, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    assert!(
        decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
            .contains("syscall(\"System.Storage.Put\");"),
        "void storage syscall should be emitted as a statement"
    );
    assert!(
        !decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
            .contains("let t0 = syscall(\"System.Storage.Put\")"),
        "void storage syscall should not push a temp onto the stack"
    );
}

#[test]
fn void_storage_local_syscall_is_emitted_as_statement() {
    // Script: SYSCALL(System.Storage.Local.Put) ; RET
    let script = [0x41, 0x39, 0x0C, 0xE3, 0x0A, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    assert!(
        decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
            .contains("syscall(\"System.Storage.Local.Put\");"),
        "void storage local syscall should be emitted as a statement"
    );
    assert!(
        !decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
            .contains("let t0 = syscall(\"System.Storage.Local.Put\")"),
        "void storage local syscall should not push a temp onto the stack"
    );
}

#[test]
fn syscall_contract_call_returns_value() {
    // Script: SYSCALL(System.Contract.Call) hash=0x525B7D62 ; RET
    let script = [0x41, 0x62, 0x7D, 0x5B, 0x52, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("syscall(\"System.Contract.Call\")"),
        "System.Contract.Call should resolve to human name: {high_level}"
    );
    assert!(
        high_level.contains("let t0 = syscall(\"System.Contract.Call\");"),
        "System.Contract.Call returns a value and should push a temp: {high_level}"
    );
}

#[test]
fn syscall_runtime_log_is_void() {
    // Script: SYSCALL(System.Runtime.Log) hash=0x9647E7CF ; RET
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
        high_level.contains("syscall(\"System.Runtime.Log\");"),
        "System.Runtime.Log is void and should be emitted as statement: {high_level}"
    );
    assert!(
        !high_level.contains("let t0 = syscall(\"System.Runtime.Log\")"),
        "void syscall should not push a temp: {high_level}"
    );
}

#[test]
fn syscall_check_witness_returns_value() {
    // Script: SYSCALL(System.Runtime.CheckWitness) hash=0x8CEC27F8 ; RET
    let script = [0x41, 0xF8, 0x27, 0xEC, 0x8C, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("syscall(\"System.Runtime.CheckWitness\")"),
        "CheckWitness should resolve to human name: {high_level}"
    );
    assert!(
        high_level.contains("let t0 = syscall(\"System.Runtime.CheckWitness\");"),
        "CheckWitness returns a value: {high_level}"
    );
}
