use super::*;
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
        high_level.contains("fn sub_0x0034(arg0, arg1)"),
        "inferred private helper at 0x0034 should be rendered separately: {high_level}"
    );
    let delegate_block = method_block(high_level, "fn testDelegate(", "\n    fn sub_0x000C(");
    assert!(
        delegate_block.contains("sub_0x0034("),
        "stored local pointer should resolve to inferred helper call in testDelegate: {delegate_block}"
    );
    assert!(
        delegate_block.contains("sub_0x0034(t2, t1)"),
        "stored local pointer call should preserve stack argument order for helper invocation: {delegate_block}"
    );
    assert!(
        !delegate_block.contains("sub_0x0034()"),
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
        csharp_delegate.contains("sub_0x0034("),
        "C# output should resolve stored local pointer CALLA target: {csharp_delegate}"
    );
    assert!(
        csharp_delegate.contains("sub_0x0034(t2, t1)")
            || csharp_delegate.contains("sub_0x0034(5, 6)"),
        "C# output should preserve CALLA helper arguments: {csharp_delegate}"
    );
    assert!(
        !csharp_delegate.contains("sub_0x0034()"),
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
        csharp_delegate.contains("StdLib.Itoa(loc1)"),
        "C# CALLT should forward token argument from stack: {csharp_delegate}"
    );
    assert!(
        csharp_delegate.contains("Helper.Concat") && csharp_delegate.contains("t_5"),
        "C# CAT should consume preserved prefix literal after CALLT arg pop: {csharp_delegate}"
    );
    assert!(
        !csharp_delegate.contains("return t4;"),
        "C# void method should not return a stray stack value: {csharp_delegate}"
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
        not_inline_case_block.contains("call_0x0106()")
            || not_inline_case_block.contains("sub_0x0106()"),
        "not_inline switch case should call the throw-tail helper at 0x0106 without arguments: {not_inline_case_block}"
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
fn initializer_anonymous_object_logs_use_emitted_getter_helpers_without_warnings() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping devpack parity test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let nef_path = artifacts_dir.join("Contract_Initializer.nef");
    let manifest_path = artifacts_dir.join("Contract_Initializer.manifest.json");
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
    let high_level_block =
        method_block(high_level, "\n    fn anonymousObjectCreation() {", "\n}\n");
    assert!(
        !high_level_block.contains("???"),
        "anonymousObjectCreation should no longer contain missing syscall placeholders: {high_level_block}"
    );
    assert!(
        high_level_block.contains("sub_0x00DA(loc0)")
            && high_level_block.contains("sub_0x00E1(t16)"),
        "anonymousObjectCreation should call the compiler-emitted anonymous getter helpers: {high_level_block}"
    );
    assert!(
        high_level_block
            .matches("syscall(\"System.Runtime.Log\"")
            .count()
            == 2,
        "anonymousObjectCreation should retain both Runtime.Log calls: {high_level_block}"
    );
    assert!(
        result.warnings.is_empty(),
        "corrected initializer artifact should decompile without warnings: {:?}",
        result.warnings
    );

    let csharp = result.csharp.as_deref().expect("csharp output");
    let csharp_block = method_block(
        csharp,
        "\n        public static void anonymousObjectCreation()",
        "\n    }\n}\n",
    );
    assert!(
        !csharp_block.contains("???"),
        "C# output should not contain missing syscall placeholders: {csharp_block}"
    );
    assert!(
        csharp.contains("sub_0x00DA(")
            && csharp.contains("sub_0x00E1(")
            && csharp.contains("Runtime.Log"),
        "C# output should render both anonymous getter helpers and Runtime.Log calls: {csharp}"
    );
}
