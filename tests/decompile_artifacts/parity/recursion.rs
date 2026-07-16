use super::*;
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
fn recursion_even_odd_uses_branch_local_value_in_recursive_call() {
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
    let even_block = method_block(
        high_level,
        "
    fn even(n: int) -> bool {",
        "
    fn odd(n: int) -> bool {",
    );
    let odd_block = method_block(
        high_level,
        "
    fn odd(n: int) -> bool {",
        "
}",
    );

    assert!(
        !even_block.contains("return odd(t6);"),
        "even() should not always call odd(t6) after the branch merge: {even_block}"
    );
    assert!(
        !odd_block.contains("return even(t6);"),
        "odd() should not always call even(t6) after the branch merge: {odd_block}"
    );
    assert!(
        even_block.contains("return odd(")
            && even_block.contains("n + 1")
            && even_block.contains("n - 1"),
        "even() should preserve both branch-specific recursive arguments: {even_block}"
    );
    assert!(
        odd_block.contains("return even(")
            && odd_block.contains("n + 1")
            && odd_block.contains("n - 1"),
        "odd() should preserve both branch-specific recursive arguments: {odd_block}"
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
        "\n    fn sub_0x00C1(arg0) -> any {",
        "\n    fn changeName(",
    );
    assert!(
        !recursion_block.contains("calla(static5)"),
        "recursive CALLA through static delegate should resolve to named internal calls: {recursion_block}"
    );
    assert!(
        recursion_block.contains("= sub_0x00C1("),
        "recursive CALLA through static delegate should render direct self-calls with arguments: {recursion_block}"
    );
}
