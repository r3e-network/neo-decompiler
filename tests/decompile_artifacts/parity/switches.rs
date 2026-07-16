use super::*;
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
