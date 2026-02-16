use std::fs;
use std::path::Path;

use neo_decompiler::{ContractManifest, Decompiler, OutputFormat};

fn method_block<'a>(text: &'a str, start_marker: &str, next_marker: &str) -> &'a str {
    let start = text
        .find(start_marker)
        .unwrap_or_else(|| panic!("missing marker `{start_marker}`"));
    let end = text[start..]
        .find(next_marker)
        .map(|relative| start + relative)
        .unwrap_or(text.len());
    &text[start..end]
}

#[test]
fn delegate_manifest_methods_do_not_swallow_private_initslot_bodies() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_Delegate.nef");
    let manifest_path = artifacts_dir.join("Contract_Delegate.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let sum_block = method_block(high_level, "fn sumFunc(", "\n    fn testDelegate(");
    assert!(
        !sum_block.contains("// 000C: INITSLOT"),
        "sumFunc should end before private helper prologue: {sum_block}"
    );
    assert!(
        high_level.contains("fn sub_0x000C(arg0, arg1)"),
        "inferred private helper at 0x000C should be rendered separately: {high_level}"
    );
    assert!(
        high_level.contains("fn sub_0x0041(arg0, arg1)"),
        "inferred private helper at 0x0041 should be rendered separately: {high_level}"
    );
    let delegate_block = method_block(high_level, "fn testDelegate(", "\n    fn sub_0x000C(");
    assert!(
        delegate_block.contains("sub_0x0041("),
        "stored local pointer should resolve to inferred helper call in testDelegate: {delegate_block}"
    );
    assert!(
        delegate_block.contains("sub_0x0041(t2, t1)"),
        "stored local pointer call should preserve stack argument order for helper invocation: {delegate_block}"
    );
    assert!(
        !delegate_block.contains("sub_0x0041()"),
        "stored local pointer call should include helper arguments in testDelegate: {delegate_block}"
    );
    assert!(
        !delegate_block.contains("calla(loc0)"),
        "resolved local pointer should not remain as generic CALLA in testDelegate: {delegate_block}"
    );

    let csharp = result.csharp.as_deref().expect("csharp output");
    let csharp_sum = method_block(
        csharp,
        "public static BigInteger sumFunc(",
        "public static void testDelegate(",
    );
    assert!(
        !csharp_sum.contains("// 000C: INITSLOT"),
        "C# sumFunc should end before private helper prologue: {csharp_sum}"
    );
    let csharp_delegate = method_block(
        csharp,
        "public static void testDelegate(",
        "private static BigInteger sub_0x000C(",
    );
    assert!(
        csharp_delegate.contains("call_0x0041("),
        "C# output should resolve stored local pointer CALLA target: {csharp_delegate}"
    );
    assert!(
        csharp_delegate.contains("call_0x0041(t2, t1)"),
        "C# output should preserve CALLA helper arguments: {csharp_delegate}"
    );
    assert!(
        !csharp_delegate.contains("call_0x0041()"),
        "C# output should not drop CALLA helper arguments: {csharp_delegate}"
    );
    assert!(
        delegate_block.contains("StdLib::Itoa(loc1)"),
        "high-level CALLT should forward token argument from stack: {delegate_block}"
    );
    assert!(
        delegate_block.contains("t4 cat t5"),
        "high-level CAT should consume preserved prefix literal after CALLT arg pop: {delegate_block}"
    );
    assert!(
        !delegate_block.contains("return t4;"),
        "high-level void method should not return a stray stack value: {delegate_block}"
    );
    assert!(
        csharp_delegate.contains("StdLib::Itoa(loc1)"),
        "C# CALLT should forward token argument from stack: {csharp_delegate}"
    );
    assert!(
        csharp_delegate.contains("t4 cat t5"),
        "C# CAT should consume preserved prefix literal after CALLT arg pop: {csharp_delegate}"
    );
    assert!(
        !csharp_delegate.contains("return t4;"),
        "C# void method should not return a stray stack value: {csharp_delegate}"
    );
}

#[test]
fn switch_jmpif_chains_use_guarded_gotos_instead_of_invalid_nested_ifs() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_Switch.nef");
    let manifest_path = artifacts_dir.join("Contract_Switch.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let switch6_block = method_block(
        high_level,
        "\n    fn switch6(method: string) -> any {",
        "\n    fn switch6Inline(",
    );
    let has_guarded_goto_shape = switch6_block.contains("if t3 { goto label_0x011C; }")
        && switch6_block.contains("label_0x011C:");
    let has_switch_shape = switch6_block.contains("switch loc0 {");
    assert!(
        has_guarded_goto_shape || has_switch_shape,
        "crossing JMPIF chain should remain in a safe structured form (guarded goto or switch): {switch6_block}"
    );
    assert!(
        !switch6_block.contains("if !t3 {"),
        "crossing branch chain should not be lifted into invalid nested negated ifs: {switch6_block}"
    );
}

#[test]
fn switch_inline_chain_is_rewritten_to_switch_cases() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_Switch.nef");
    let manifest_path = artifacts_dir.join("Contract_Switch.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let switch6_inline = method_block(
        high_level,
        "\n    fn switch6Inline(method: string) -> any {",
        "\n    fn switchInteger(",
    );
    assert!(
        switch6_inline.contains("switch loc0 {"),
        "switch6Inline should be reconstructed as switch/case: {switch6_inline}"
    );
    assert!(
        switch6_inline.contains("case \"0\" {"),
        "switch6Inline should include case \"0\": {switch6_inline}"
    );
    assert!(
        switch6_inline.contains("case \"5\" {"),
        "switch6Inline should include case \"5\": {switch6_inline}"
    );
}

#[test]
fn switch_long_guarded_goto_chain_is_rewritten_to_switch_cases() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_Switch.nef");
    let manifest_path = artifacts_dir.join("Contract_Switch.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let switch_long = method_block(
        high_level,
        "\n    fn switchLong(method: string) -> any {",
        "\n    fn switch6(",
    );
    assert!(
        switch_long.contains("switch loc0 {"),
        "switchLong should be reconstructed as switch/case: {switch_long}"
    );
    assert!(
        switch_long.contains("case \"0\" {"),
        "switchLong should preserve first case literal: {switch_long}"
    );
    assert!(
        switch_long.contains("case \"20\" {"),
        "switchLong should preserve last case literal: {switch_long}"
    );
    assert!(
        switch_long.contains("default {"),
        "switchLong should preserve default branch: {switch_long}"
    );
    assert!(
        !switch_long.contains("if t1 { goto label_0x00B7; }"),
        "guarded goto chain should be replaced by switch/case structure: {switch_long}"
    );
}

#[test]
fn switch6_guarded_chain_is_rewritten_to_switch_cases() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_Switch.nef");
    let manifest_path = artifacts_dir.join("Contract_Switch.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let switch6_block = method_block(
        high_level,
        "\n    fn switch6(method: string) -> any {",
        "\n    fn switch6Inline(",
    );
    assert!(
        switch6_block.contains("switch loc0 {"),
        "switch6 should be reconstructed as switch/case: {switch6_block}"
    );
    assert!(
        switch6_block.contains("case \"0\" {"),
        "switch6 should preserve first case literal: {switch6_block}"
    );
    assert!(
        switch6_block.contains("case \"5\" {"),
        "switch6 should preserve last explicit case literal: {switch6_block}"
    );
    assert!(
        switch6_block.contains("default {"),
        "switch6 should preserve default branch: {switch6_block}"
    );
}

#[test]
fn switch_long_long_rewrite_keeps_case_and_default_blocks_well_formed() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_Switch.nef");
    let manifest_path = artifacts_dir.join("Contract_Switch.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let switch_long_long = method_block(
        high_level,
        "\n    fn switchLongLong(test: string) -> any {",
        "\n}",
    );
    assert!(
        switch_long_long.contains("switch loc1 {"),
        "switchLongLong should remain a switch on the string scrutinee: {switch_long_long}"
    );
    assert!(
        switch_long_long.contains("case \"g\" {"),
        "switchLongLong should include case \"g\": {switch_long_long}"
    );
    assert!(
        switch_long_long.contains("default {"),
        "switchLongLong should include default branch: {switch_long_long}"
    );
    assert!(
        !switch_long_long.contains("case \"g\" {\n                else {"),
        "switchLongLong case bodies must not contain dangling else-block wrappers: {switch_long_long}"
    );
}

#[test]
fn foreach_contract_methods_use_structured_loops_without_unlifted_cfg_warnings() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_Foreach.nef");
    let manifest_path = artifacts_dir.join("Contract_Foreach.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let int_foreach_block = method_block(
        high_level,
        "\n    fn intForeach() -> int {",
        "\n    fn stringForeach(",
    );
    assert!(
        !int_foreach_block.contains("control flow not yet lifted"),
        "intForeach should not include unlifted CFG warning comments after loop reconstruction: {int_foreach_block}"
    );
    assert!(
        int_foreach_block.contains("for (") || int_foreach_block.contains("while "),
        "intForeach should use a structured loop form: {int_foreach_block}"
    );
}

#[test]
fn foreach_pack_helpers_do_not_emit_literal_pack_underflow_warnings() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_Foreach.nef");
    let manifest_path = artifacts_dir.join("Contract_Foreach.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let helper_049e_block = method_block(high_level, "\n    fn sub_0x049E() {", "\n}");
    assert!(
        !helper_049e_block.contains("// 049F: insufficient values on stack for PACKSTRUCT (needs 2)"),
        "sub_0x049E should model PACKSTRUCT at 0x049F without underflow warnings: {helper_049e_block}"
    );
    assert!(
        !helper_049e_block.contains("// 04A1: insufficient values on stack for PACKSTRUCT (needs 2)"),
        "sub_0x049E should model PACKSTRUCT at 0x04A1 without underflow warnings: {helper_049e_block}"
    );
    assert!(
        !helper_049e_block.contains("// 04A3: insufficient values on stack for PACK (needs 2)"),
        "sub_0x049E should model PACK at 0x04A3 without underflow warnings: {helper_049e_block}"
    );
    assert!(
        !high_level.contains("// 04B8: insufficient values on stack for STLOC4 (needs 1)"),
        "PACK placeholder modeling should not regress the STLOC4 stack check at 0x04B8: {high_level}"
    );
}

#[test]
fn inline_not_inline_case_does_not_require_spurious_call_argument() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_Inline.nef");
    let manifest_path = artifacts_dir.join("Contract_Inline.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let not_inline_case_block = method_block(
        high_level,
        "case \"not_inline\" {",
        "case \"not_inline_with_one_parameters\" {",
    );
    assert!(
        !not_inline_case_block.contains("// 0099: insufficient values on stack for CALL (needs 1)"),
        "not_inline switch case should not require a synthetic argument for CALL at 0x0099: {not_inline_case_block}"
    );
    assert!(
        not_inline_case_block.contains("call_0x010D()")
            || not_inline_case_block.contains("sub_0x010D()"),
        "not_inline switch case should call the throw-tail helper at 0x010D without arguments: {not_inline_case_block}"
    );
}

#[test]
fn trycatch_handlers_do_not_underflow_on_catch_exception_slot_store() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_TryCatch.nef");
    let manifest_path = artifacts_dir.join("Contract_TryCatch.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let try01_block = method_block(
        high_level,
        "\n    fn try01(throwException: bool, enterCatch: bool, enterFinally: bool) -> int {",
        "\n    fn try02(",
    );
    assert!(
        !try01_block.contains("insufficient values on stack for STLOC1"),
        "catch-entry exception value should be available for STLOC1 in try01: {try01_block}"
    );
}

#[test]
fn trycatch_contract_has_no_stack_underflow_warnings_after_catch_stack_modeling() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_TryCatch.nef");
    let manifest_path = artifacts_dir.join("Contract_TryCatch.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    assert!(
        !high_level.contains("insufficient values on stack"),
        "try/catch stack modeling should eliminate stack-underflow warnings in Contract_TryCatch: {high_level}"
    );
}

#[test]
fn nep11_balance_of_istype_and_unpack_stack_modeling_avoids_underflow_warnings() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_NEP11.nef");
    let manifest_path = artifacts_dir.join("Contract_NEP11.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let balance_of_block = method_block(
        high_level,
        "\n    fn balanceOf(owner: hash160) -> int {",
        "\n    fn ownerOf(",
    );
    assert!(
        !balance_of_block.contains("is_type(owner, t0)"),
        "ISTYPE should consume a single value and use operand type metadata, not a duplicated stack value: {balance_of_block}"
    );
    assert!(
        !balance_of_block.contains("insufficient values on stack"),
        "balanceOf should not emit stack-underflow warnings once ISTYPE/UNPACK stack modeling is correct: {balance_of_block}"
    );
    assert!(
        !balance_of_block.contains("syscall(\"System.Storage.Get\", ???"),
        "UNPACK stack modeling should preserve the storage context argument for Storage.Get in balanceOf: {balance_of_block}"
    );
}

#[test]
fn write_in_try_internal_calls_prefer_symbolic_targets_over_raw_offsets() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_WriteInTry.nef");
    let manifest_path = artifacts_dir.join("Contract_WriteInTry.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let mutual_recursive_try_block = method_block(
        high_level,
        "\n    fn mutualRecursiveTry(i: int) {",
        "\n    fn safeTryWithCatchWithThrowInFinally(",
    );
    assert!(
        mutual_recursive_try_block.contains("recursiveTry("),
        "mutualRecursiveTry should resolve manifest recursive call target by name: {mutual_recursive_try_block}"
    );
    assert!(
        mutual_recursive_try_block.contains("tryWriteWithVulnerability()"),
        "mutualRecursiveTry should resolve manifest call target by name: {mutual_recursive_try_block}"
    );
    assert!(
        !mutual_recursive_try_block.contains("call_0x00AA"),
        "mutualRecursiveTry should normalize interior target 0x00AA to recursiveTry entry: {mutual_recursive_try_block}"
    );
    assert!(
        !mutual_recursive_try_block.contains("call_0x009C"),
        "mutualRecursiveTry should normalize interior target 0x009C to tryWriteWithVulnerability entry: {mutual_recursive_try_block}"
    );
}

#[test]
fn property_setters_without_initslot_keep_method_boundaries_and_stack_entry_args() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_Property.nef");
    let manifest_path = artifacts_dir.join("Contract_Property.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let set_public_static_property_block = method_block(
        high_level,
        "\n    fn setPublicStaticProperty(value: string) {",
        "\n    fn uninitializedStaticProperty()",
    );
    assert!(
        !set_public_static_property_block.contains("insufficient values on stack"),
        "setPublicStaticProperty should consume entry-stack argument without underflow: {set_public_static_property_block}"
    );
    assert!(
        !set_public_static_property_block.contains("PICKITEM"),
        "setPublicStaticProperty should not absorb neighboring helper methods after RET: {set_public_static_property_block}"
    );

    let set_uninitialized_static_property_block = method_block(
        high_level,
        "\n    fn setUninitializedStaticProperty(value: int) {",
        "\n    fn testStaticPropertyInc()",
    );
    assert!(
        !set_uninitialized_static_property_block.contains("insufficient values on stack"),
        "setUninitializedStaticProperty should consume entry-stack argument without underflow: {set_uninitialized_static_property_block}"
    );

    let csharp = result.csharp.as_deref().expect("csharp output");
    let csharp_set_public_static_property_block = method_block(
        csharp,
        "public static void setPublicStaticProperty(",
        "public static BigInteger uninitializedStaticProperty(",
    );
    assert!(
        !csharp_set_public_static_property_block.contains("insufficient values on stack"),
        "C# setPublicStaticProperty should not emit stack underflow warnings: {csharp_set_public_static_property_block}"
    );
    assert!(
        !csharp_set_public_static_property_block.contains("// 0062: PICKITEM"),
        "C# setPublicStaticProperty should not absorb helper chunk at 0x0061: {csharp_set_public_static_property_block}"
    );

    let csharp_set_uninitialized_static_property_block = method_block(
        csharp,
        "public static void setUninitializedStaticProperty(",
        "public static BigInteger testStaticPropertyInc(",
    );
    assert!(
        !csharp_set_uninitialized_static_property_block.contains("insufficient values on stack"),
        "C# setUninitializedStaticProperty should not emit stack underflow warnings: {csharp_set_uninitialized_static_property_block}"
    );
    assert!(
        !csharp.contains("insufficient values on stack"),
        "Contract_Property C# output should not contain stack-underflow diagnostics: {csharp}"
    );
}

#[test]
fn null_contract_else_paths_keep_stack_shape_for_reverse4_sequences() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_NULL.nef");
    let manifest_path = artifacts_dir.join("Contract_NULL.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let null_coalesce_assign_block = method_block(
        high_level,
        "\n    fn nullCoalescingAssignment(nullableArg: int) {",
        "\n    fn staticNullableCoalesceAssignment()",
    );
    assert!(
        !null_coalesce_assign_block.contains("insufficient values on stack for REVERSE4"),
        "nullCoalescingAssignment else-path stack should be restored before REVERSE4 rewrites: {null_coalesce_assign_block}"
    );
    assert!(
        !null_coalesce_assign_block.contains("insufficient values on stack for DROP"),
        "nullCoalescingAssignment should not leave dangling stack underflow on post-merge DROP: {null_coalesce_assign_block}"
    );
}

#[test]
fn nullconditional_post_ret_helpers_split_and_avoid_stack_underflow() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_NullConditional.nef");
    let manifest_path = artifacts_dir.join("Contract_NullConditional.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    assert!(
        high_level.contains("\n    fn sub_0x0032("),
        "post-RET chunk at 0x0032 should be inferred as a helper method start: {high_level}"
    );
    assert!(
        high_level.contains("\n    fn sub_0x00A8("),
        "post-RET chunk at 0x00A8 should be inferred as a helper method start: {high_level}"
    );
    assert!(
        high_level.contains("\n    fn sub_0x00AC("),
        "post-RET chunk at 0x00AC should be inferred as a helper method start: {high_level}"
    );

    let assign_child_block = method_block(
        high_level,
        "\n    fn assignChild(createNode: bool) -> int {",
        "\n    fn sub_0x0032(",
    );
    assert!(
        !assign_child_block.contains("insufficient values on stack"),
        "assignChild should not absorb trailing helper chunk stack underflow warnings: {assign_child_block}"
    );
    assert!(
        !assign_child_block.contains("// 0032: PUSH0"),
        "assignChild should end before post-RET helper prologue at 0x0032: {assign_child_block}"
    );

    let assign_static_block = method_block(
        high_level,
        "\n    fn assignStatic(createNode: bool) -> int {",
        "\n    fn sub_0x00A8(",
    );
    assert!(
        !assign_static_block.contains("insufficient values on stack"),
        "assignStatic should not absorb post-RET helper chunks that underflow at 0x00A8/0x00AC: {assign_static_block}"
    );
    assert!(
        !assign_static_block.contains("// 00A8: STSFLD0"),
        "assignStatic should end before post-RET helper prologue at 0x00A8: {assign_static_block}"
    );

    let helper_0032_block = method_block(
        high_level,
        "\n    fn sub_0x0032(",
        "\n    fn assignSibling(",
    );
    assert!(
        !helper_0032_block.contains("insufficient values on stack"),
        "split helper starting at 0x0032 should receive entry-stack arguments without underflow: {helper_0032_block}"
    );

    let helper_00a8_block = method_block(
        high_level,
        "\n    fn sub_0x00A8(",
        "\n    fn assignGrandChild(",
    );
    assert!(
        !helper_00a8_block.contains("insufficient values on stack"),
        "split helper chunks at 0x00A8/0x00AC should not emit stack-underflow warnings: {helper_00a8_block}"
    );

    let csharp = result.csharp.as_deref().expect("csharp output");
    let csharp_assign_static_block = method_block(
        csharp,
        "public static BigInteger assignStatic(",
        "public static BigInteger assignGrandChild(",
    );
    assert!(
        !csharp_assign_static_block.contains("insufficient values on stack"),
        "C# assignStatic should not emit stack-underflow warnings: {csharp_assign_static_block}"
    );
    assert!(
        !csharp_assign_static_block.contains("// 00A8: STSFLD0"),
        "C# assignStatic should end before detached helper chunk at 0x00A8: {csharp_assign_static_block}"
    );
    assert!(
        !csharp.contains("insufficient values on stack"),
        "Contract_NullConditional C# output should not contain stack-underflow diagnostics: {csharp}"
    );
}

#[test]
fn property_inferred_helpers_without_initslot_receive_entry_stack_arguments() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_Property.nef");
    let manifest_path = artifacts_dir.join("Contract_Property.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let helper_0061_block =
        method_block(high_level, "\n    fn sub_0x0061(", "\n    fn sub_0x0064(");
    assert!(
        !helper_0061_block.contains("insufficient values on stack"),
        "sub_0x0061 should receive synthetic entry-stack args for PICKITEM getter wrapper: {helper_0061_block}"
    );

    let helper_0064_block =
        method_block(high_level, "\n    fn sub_0x0064(", "\n    fn sub_0x0068(");
    assert!(
        !helper_0064_block.contains("insufficient values on stack"),
        "sub_0x0064 should receive synthetic entry-stack args for SETITEM setter wrapper: {helper_0064_block}"
    );

    let helper_0068_block =
        method_block(high_level, "\n    fn sub_0x0068(", "\n    fn sub_0x006B(");
    assert!(
        !helper_0068_block.contains("insufficient values on stack"),
        "sub_0x0068 should receive synthetic entry-stack args for PICKITEM getter wrapper: {helper_0068_block}"
    );

    let helper_006b_block =
        method_block(high_level, "\n    fn sub_0x006B(", "\n    fn sub_0x0074(");
    assert!(
        !helper_006b_block.contains("insufficient values on stack"),
        "sub_0x006B should receive synthetic entry-stack args for SETITEM setter wrapper: {helper_006b_block}"
    );
}

#[test]
fn recursion_internal_calls_preserve_argument_expressions() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_Recursion.nef");
    let manifest_path = artifacts_dir.join("Contract_Recursion.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let factorial_block = method_block(
        high_level,
        "\n    fn factorial(a: int) -> int {",
        "\n    fn hanoiTower(",
    );
    assert!(
        !factorial_block.contains("factorial();"),
        "recursive factorial call should carry argument expression: {factorial_block}"
    );
    assert!(
        factorial_block.contains("factorial(t5)"),
        "recursive factorial call should forward decremented argument expression: {factorial_block}"
    );
    let hanoi_block = method_block(
        high_level,
        "\n    fn hanoiTower(n: int, src: int, aux: int, dst: int) -> array {",
        "\n    fn even(",
    );
    assert!(
        !hanoi_block.contains("hanoiTower();"),
        "recursive hanoi calls should carry argument expressions: {hanoi_block}"
    );
    assert!(
        hanoi_block.contains("hanoiTower(t9, src, dst, aux)"),
        "first recursive hanoi call should preserve argument ordering: {hanoi_block}"
    );
    assert!(
        hanoi_block.contains("hanoiTower(t13, aux, src, dst)"),
        "second recursive hanoi call should preserve argument ordering: {hanoi_block}"
    );
    let even_block = method_block(
        high_level,
        "\n    fn even(n: int) -> bool {",
        "\n    fn odd(",
    );
    assert!(
        !even_block.contains("odd();"),
        "mutual recursion call should carry argument expression in even(): {even_block}"
    );
    let odd_block = method_block(high_level, "\n    fn odd(n: int) -> bool {", "\n}");
    assert!(
        !odd_block.contains("even();"),
        "mutual recursion call should carry argument expression in odd(): {odd_block}"
    );
}

#[test]
fn lambda_static_delegate_recursion_resolves_to_internal_calls() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_Lambda.nef");
    let manifest_path = artifacts_dir.join("Contract_Lambda.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let recursion_block = method_block(
        high_level,
        "\n    fn sub_0x00DB(arg0) {",
        "\n    fn changeName(",
    );
    assert!(
        !recursion_block.contains("calla(static5)"),
        "recursive CALLA through static delegate should resolve to named internal calls: {recursion_block}"
    );
    assert!(
        recursion_block.contains("= sub_0x00DB("),
        "recursive CALLA through static delegate should render direct self-calls with arguments: {recursion_block}"
    );
}

#[test]
fn reentrancy_unknown_unpack_preserves_stack_for_reverse3_swap_helpers() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_Reentrancy.nef");
    let manifest_path = artifacts_dir.join("Contract_Reentrancy.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    assert!(
        high_level.contains("\n    fn sub_0x0268(arg0) {"),
        "expected helper block sub_0x0268 to be present: {high_level}"
    );
    assert!(
        high_level.contains("\n    fn sub_0x02A4(arg0) {"),
        "expected helper block sub_0x02A4 to be present: {high_level}"
    );
    assert!(
        !high_level.contains("// 0273: insufficient values on stack for REVERSE3 (needs 3)"),
        "sub_0x0268 should keep enough UNPACK-derived stack entries for REVERSE3 at 0x0273: {high_level}"
    );
    assert!(
        !high_level.contains("// 0275: insufficient values on stack for SWAP (needs 2)"),
        "sub_0x0268 should keep enough UNPACK-derived stack entries for SWAP at 0x0275: {high_level}"
    );
    assert!(
        !high_level.contains("// 02AF: insufficient values on stack for REVERSE3 (needs 3)"),
        "sub_0x02A4 should keep enough UNPACK-derived stack entries for REVERSE3 at 0x02AF: {high_level}"
    );
    assert!(
        !high_level.contains("// 02B1: insufficient values on stack for SWAP (needs 2)"),
        "sub_0x02A4 should keep enough UNPACK-derived stack entries for SWAP at 0x02B1: {high_level}"
    );
}

#[test]
fn tuple_unknown_unpack_preserves_stack_for_drop_stloc_drop_sequence() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_Tuple.nef");
    let manifest_path = artifacts_dir.join("Contract_Tuple.manifest.json");
    if !nef_path.is_file() || !manifest_path.is_file() {
        eprintln!(
            "Skipping devpack parity test: missing {} or {}",
            nef_path.display(),
            manifest_path.display()
        );
        return;
    }

    let nef_bytes = fs::read(&nef_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", nef_path.display()));
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_json)
        .unwrap_or_else(|err| panic!("invalid manifest {}: {err}", manifest_path.display()));

    let result = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    assert!(
        high_level.contains("\n    fn t1() -> any {"),
        "expected tuple method body to be present: {high_level}"
    );
    assert!(
        !high_level.contains("// 002D: insufficient values on stack for DROP (needs 1)"),
        "UNPACK should preserve enough stack entries for DROP at 0x002D in Contract_Tuple::t1: {high_level}"
    );
    assert!(
        !high_level.contains("// 002E: insufficient values on stack for STLOC1 (needs 1)"),
        "UNPACK should preserve enough stack entries for STLOC1 at 0x002E in Contract_Tuple::t1: {high_level}"
    );
    assert!(
        !high_level.contains("// 002F: insufficient values on stack for DROP (needs 1)"),
        "UNPACK should preserve enough stack entries for DROP at 0x002F in Contract_Tuple::t1: {high_level}"
    );
}
