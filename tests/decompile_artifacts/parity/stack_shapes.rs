use super::*;
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
        .with_inline_single_use_temps(true)
        .with_trace_comments(false)
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
        int_foreach_block.contains("for ("),
        "intForeach should recover its compiler-generated induction loop as a for loop: {int_foreach_block}"
    );
    let int_forloop_block = method_block(
        high_level,
        "\n    fn intForloop() -> int {",
        "\n    fn testIteratorForEach(",
    );
    assert!(
        int_forloop_block.contains("for (") || int_forloop_block.contains("while "),
        "intForloop should recover its scalar-normalized induction loop as a structured loop: {int_forloop_block}"
    );
    let csharp = result.csharp.as_deref().expect("C# output");
    let csharp_int_forloop = method_block(
        csharp,
        "public static BigInteger intForloop()",
        "\n        public static void testIteratorForEach(",
    );
    assert!(
        csharp_int_forloop.contains("for ("),
        "C# intForloop should recover its scalar-normalized induction loop as a for loop: {csharp_int_forloop}"
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
        .with_trace_comments(false)
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = result.high_level.as_deref().expect("high-level output");
    let helper_marker = ["\n    fn sub_0x0450("]
        .into_iter()
        .find(|marker| high_level.contains(marker))
        .expect("Foreach tuple helper marker");
    let helper_block = method_block(high_level, helper_marker, "\n}");
    assert!(
        !helper_block.contains("insufficient values on stack for PACKSTRUCT"),
        "Foreach tuple helper should model PACKSTRUCT without underflow warnings: {helper_block}"
    );
    assert!(
        !helper_block.contains("insufficient values on stack for PACK (needs 2)"),
        "Foreach tuple helper should model PACK without underflow warnings: {helper_block}"
    );
    assert!(
        !high_level.contains("insufficient values on stack for STLOC4 (needs 1)"),
        "PACK placeholder modeling should not regress the STLOC4 stack check at 0x046B: {high_level}"
    );
    assert!(
        !helper_block.contains("missing_pack_item()"),
        "Foreach tuple helper should infer entry-stack values for PACK/PACKSTRUCT instead of synthesizing missing pack items: {helper_block}"
    );
}

#[test]
fn foreach_tuple_helper_underflow_stays_explicit_and_compile_safe() {
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
        .with_inline_single_use_temps(true)
        .with_trace_comments(false)
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    assert!(
        result.warnings.iter().any(
            |warning| warning.contains("045B: insufficient values on stack for CALL (needs 4)")
        ),
        "tuple helper underflow should remain visible in decompilation warnings: {:?}",
        result.warnings
    );
    let csharp = result.csharp.as_deref().expect("C# output");
    let method = method_block(
        csharp,
        "public static void testForEachVariable()",
        "\n        public static void testDo()",
    );
    assert!(
        method.contains("VM argument underflow in testForEachVariable at 0x045B"),
        "C# should retain an honest underflow comment: {method}"
    );
    assert!(
        method.contains(
            "sub_0x0450((dynamic)(((object)null) ?? throw new InvalidOperationException(\"VM argument underflow"
        ),
        "C# should use compile-safe throwing expressions for unproven call arguments: {method}"
    );
    assert!(
        method.contains("dynamic loc3 = (dynamic)null;")
            && method.contains("dynamic loc4 = (dynamic)null;"),
        "C# should keep unrelated unknown tuple elements as compatibility nulls: {method}"
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
        high_level.contains("\n    fn sub_0x0272(arg0) {"),
        "expected helper block sub_0x0272 to be present: {high_level}"
    );
    assert!(
        high_level.contains("\n    fn sub_0x02AE(arg0) {"),
        "expected helper block sub_0x02AE to be present: {high_level}"
    );
    assert!(
        !high_level.contains("// 027D: insufficient values on stack for REVERSE3 (needs 3)"),
        "sub_0x0272 should keep enough UNPACK-derived stack entries for REVERSE3 at 0x027D: {high_level}"
    );
    assert!(
        !high_level.contains("// 027F: insufficient values on stack for SWAP (needs 2)"),
        "sub_0x0272 should keep enough UNPACK-derived stack entries for SWAP at 0x027F: {high_level}"
    );
    assert!(
        !high_level.contains("// 02B9: insufficient values on stack for REVERSE3 (needs 3)"),
        "sub_0x02AE should keep enough UNPACK-derived stack entries for REVERSE3 at 0x02B9: {high_level}"
    );
    assert!(
        !high_level.contains("// 02BB: insufficient values on stack for SWAP (needs 2)"),
        "sub_0x02AE should keep enough UNPACK-derived stack entries for SWAP at 0x02BB: {high_level}"
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
