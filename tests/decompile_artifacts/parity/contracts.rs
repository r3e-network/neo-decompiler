use super::*;
#[test]
fn edgecase_csharp_output_stays_high_level() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/edgecases");
    let artifacts = [
        ("LoopIf.nef", "LoopIf.manifest.json"),
        ("events/Events.nef", "events/Events.manifest.json"),
        ("multi/MultiMethod.nef", "multi/MultiMethod.manifest.json"),
        (
            "permissions/Permissions.nef",
            "permissions/Permissions.manifest.json",
        ),
    ];

    for (nef_name, manifest_name) in artifacts {
        let nef_path = root.join(nef_name);
        let manifest_path = root.join(manifest_name);
        if !nef_path.is_file() || !manifest_path.is_file() {
            eprintln!("Skipping missing edgecase artifact {}", nef_path.display());
            continue;
        }
        let nef = fs::read(&nef_path).expect("read edgecase NEF");
        let manifest = ContractManifest::from_json_str(
            &fs::read_to_string(&manifest_path).expect("read edgecase manifest"),
        )
        .expect("parse edgecase manifest");
        let result = Decompiler::new()
            .with_inline_single_use_temps(true)
            .with_trace_comments(false)
            .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::CSharp)
            .expect("edgecase C# decompilation succeeds");
        let csharp = result.csharp.as_deref().expect("C# output");
        let has_temp_declaration = csharp.lines().any(|line| {
            let statement = line.trim_start();
            [
                "var t",
                "dynamic t",
                "BigInteger t",
                "ByteString t",
                "object t",
                "bool t",
            ]
            .iter()
            .any(|prefix| statement.starts_with(prefix))
        });
        assert!(
            !csharp.contains("Runtime.LoadScript") && !has_temp_declaration,
            "{} regressed to VM-shaped C# output:\n{csharp}",
            nef_path.display()
        );
    }
}

/// Every supported in-repo NEF + companion manifest must decompile to a
/// human-readable N3 `SmartContract` C# skeleton: envelope, ABI methods with
/// braced bodies, pattern confidence header, and no defeated LoopIf shape.
#[test]
fn all_supported_artifacts_decompile_to_high_level_csharp_contracts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts");
    let expected_invalid = root.join("expected_invalid.txt");
    let known_unsupported = root.join("known_unsupported.txt");
    let skip_substrings: Vec<String> = [expected_invalid, known_unsupported]
        .into_iter()
        .filter(|path| path.is_file())
        .flat_map(|path| {
            fs::read_to_string(path)
                .unwrap_or_default()
                .lines()
                .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
                .map(|line| {
                    line.split(':')
                        .next()
                        .unwrap_or(line)
                        .trim()
                        .trim_end_matches('/')
                        .to_string()
                })
                .collect::<Vec<_>>()
        })
        .collect();

    let mut nef_paths = Vec::new();
    fn collect_nefs(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip generated decompiled tree except embedded sample NEF parent.
                if path.file_name().and_then(|n| n.to_str()) == Some("decompiled") {
                    let embedded = path.join("embedded");
                    if embedded.is_dir() {
                        collect_nefs(&embedded, out);
                    }
                    continue;
                }
                collect_nefs(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("nef") {
                out.push(path);
            }
        }
    }
    collect_nefs(&root, &mut nef_paths);
    nef_paths.sort();
    assert!(
        !nef_paths.is_empty(),
        "expected at least one NEF under TestingArtifacts"
    );

    let mut checked = 0usize;
    for nef_path in &nef_paths {
        let rel = nef_path
            .strip_prefix(&root)
            .unwrap_or(nef_path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        if skip_substrings
            .iter()
            .any(|skip| rel.contains(skip) || rel.contains(&skip.replace('\\', "/")))
        {
            continue;
        }

        let stem = nef_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let manifest_path = nef_path.with_file_name(format!("{stem}.manifest.json"));
        assert!(
            manifest_path.is_file(),
            "companion manifest missing for {}",
            nef_path.display()
        );

        let nef = fs::read(nef_path).unwrap_or_else(|e| panic!("read {}: {e}", nef_path.display()));
        let manifest = ContractManifest::from_json_str(
            &fs::read_to_string(&manifest_path)
                .unwrap_or_else(|e| panic!("read {}: {e}", manifest_path.display())),
        )
        .unwrap_or_else(|e| panic!("parse {}: {e}", manifest_path.display()));

        let result = Decompiler::new()
            .with_inline_single_use_temps(true)
            .with_trace_comments(false)
            .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
            .unwrap_or_else(|e| panic!("decompile {}: {e}", nef_path.display()));

        let csharp = result
            .csharp
            .as_deref()
            .unwrap_or_else(|| panic!("missing C# for {}", nef_path.display()));
        let high_level = result
            .high_level
            .as_deref()
            .unwrap_or_else(|| panic!("missing high-level for {}", nef_path.display()));

        assert!(
            csharp.contains(": SmartContract"),
            "{} missing SmartContract class:\n{csharp}",
            nef_path.display()
        );
        assert!(
            csharp.contains("public static "),
            "{} missing public static methods:\n{csharp}",
            nef_path.display()
        );
        assert!(
            csharp.contains("pattern confidence:"),
            "{} missing pattern confidence header:\n{csharp}",
            nef_path.display()
        );
        assert!(
            csharp.contains('{') && csharp.contains('}'),
            "{} C# must include braced method bodies:\n{csharp}",
            nef_path.display()
        );
        assert!(
            !csharp.contains("while (true)") || !rel.contains("LoopIf"),
            "{} LoopIf must not keep while(true):\n{csharp}",
            nef_path.display()
        );
        assert!(
            high_level.contains("contract ") && high_level.contains("fn "),
            "{} high-level must keep contract envelope:\n{high_level}",
            nef_path.display()
        );
        if rel.contains("events/Events") {
            assert!(
                csharp.contains("public static event "),
                "Events fixture must declare events:\n{csharp}"
            );
            assert!(
                csharp.contains("inferred patterns: events"),
                "Events fixture must surface events pattern:\n{csharp}"
            );
        }
        if rel.contains("multi/MultiMethod") {
            assert!(
                csharp.contains("main(") && csharp.contains("helper("),
                "MultiMethod must keep ABI method names:\n{csharp}"
            );
        }
        if rel.contains("LoopIf") {
            assert!(
                csharp.contains("for (") || csharp.contains("while ("),
                "LoopIf must recover a structured loop:\n{csharp}"
            );
            assert!(
                !high_level.contains("loop {"),
                "LoopIf high-level must not keep infinite loop form:\n{high_level}"
            );
        }
        checked += 1;
    }
    assert!(
        checked >= 6,
        "expected to validate at least the six supported edge/embedded fixtures, checked {checked}"
    );
}
