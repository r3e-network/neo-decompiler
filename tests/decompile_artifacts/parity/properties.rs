use super::*;
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
        high_level.contains("\n    fn sub_0x003E("),
        "post-RET chunk at 0x003E should be inferred as a helper method start: {high_level}"
    );
    assert!(
        high_level.contains("\n    fn sub_0x0071("),
        "post-RET chunk at 0x0071 should be inferred as a helper method start: {high_level}"
    );

    let assign_child_block = method_block(
        high_level,
        "\n    fn assignChild(createNode: bool) -> int {",
        "\n    fn assignSibling(createNode: bool) -> int {",
    );
    assert!(
        !assign_child_block.contains("insufficient values on stack"),
        "assignChild should not absorb trailing helper chunk stack underflow warnings: {assign_child_block}"
    );
    assert!(
        !assign_child_block.contains("// 00E2: INITSLOT"),
        "assignChild should end before assignSibling at 0x00E2: {assign_child_block}"
    );

    let assign_static_block = method_block(
        high_level,
        "\n    fn assignStatic(createNode: bool) -> int {",
        "\n    fn assignGrandChild(createRoot: bool, createChild: bool) -> int {",
    );
    assert!(
        !assign_static_block.contains("insufficient values on stack"),
        "assignStatic should not absorb detached helper chunks after 0x0169: {assign_static_block}"
    );
    assert!(
        !assign_static_block.contains("// 016A: INITSLOT"),
        "assignStatic should end before assignGrandChild at 0x016A: {assign_static_block}"
    );

    let helper_003e_block =
        method_block(high_level, "\n    fn sub_0x003E(", "\n    fn sub_0x0071(");
    assert!(
        !helper_003e_block.contains("insufficient values on stack"),
        "split helper starting at 0x003E should receive entry-stack arguments without underflow: {helper_003e_block}"
    );

    let helper_0071_block =
        method_block(high_level, "\n    fn sub_0x0071(", "\n    fn sub_0x0B13(");
    assert!(
        !helper_0071_block.contains("insufficient values on stack"),
        "split helper starting at 0x0071 should receive entry-stack arguments without underflow: {helper_0071_block}"
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
        !csharp_assign_static_block.contains("// 016A: INITSLOT"),
        "C# assignStatic should end before assignGrandChild at 0x016A: {csharp_assign_static_block}"
    );
    let csharp_assign_sibling_block = method_block(
        csharp,
        "public static BigInteger assignSibling(",
        "public static BigInteger assignStatic(",
    );
    assert!(
        !csharp_assign_sibling_block.contains("if (false)"),
        "C# assignSibling must not fold a nullable object-array value to false: {csharp_assign_sibling_block}"
    );
    assert!(
        csharp_assign_sibling_block.contains("is null")
            && csharp_assign_sibling_block.contains("return 0;")
            && csharp_assign_sibling_block.contains("return 1;"),
        "C# assignSibling should preserve both outcomes of its null check: {csharp_assign_sibling_block}"
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
