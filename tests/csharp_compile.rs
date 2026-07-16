#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use neo_decompiler::{ContractManifest, Decompiler, OutputFormat};
use tempfile::tempdir;

const PINNED_DEVPACK_COMMIT: &str = "5b0b63880b6201ae3f974cc845e93a90462d8043";

fn framework_config() -> (PathBuf, String) {
    let framework = PathBuf::from(
        env::var_os("NEO_SMARTCONTRACT_FRAMEWORK_DLL")
            .expect("NEO_SMARTCONTRACT_FRAMEWORK_DLL is required for csharp_compile"),
    );
    assert!(
        framework.is_file(),
        "NEO_SMARTCONTRACT_FRAMEWORK_DLL is not a file: {}",
        framework.display()
    );
    let target_framework =
        env::var("NEO_CSHARP_TARGET_FRAMEWORK").unwrap_or_else(|_| "net8.0".to_string());
    assert!(
        target_framework.starts_with("net")
            && target_framework.len() > 3
            && target_framework[3..]
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '.'),
        "NEO_CSHARP_TARGET_FRAMEWORK must be a simple target moniker such as net8.0"
    );
    (framework, target_framework)
}

fn write_project(project: &Path, framework: &Path, target_framework: &str) {
    fs::write(
        project.join("Generated.csproj"),
        format!(
            "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>{target_framework}</TargetFramework><Nullable>disable</Nullable><ImplicitUsings>disable</ImplicitUsings><ErrorLog>diagnostics.sarif,version=2.1</ErrorLog></PropertyGroup><ItemGroup><Reference Include=\"Neo.SmartContract.Framework\"><HintPath>{}</HintPath></Reference></ItemGroup></Project>",
            framework.to_string_lossy()
        ),
    )
    .expect("write project");
}

fn build_project(project: &Path) -> Output {
    Command::new("dotnet")
        .args(["build", "--nologo", "--verbosity", "quiet"])
        .current_dir(project)
        .output()
        .expect("run dotnet build")
}

fn restore_project(project: &Path) {
    let output = Command::new("dotnet")
        .args(["restore", "--nologo", "--verbosity", "quiet"])
        .current_dir(project)
        .output()
        .expect("run dotnet restore");
    assert!(
        output.status.success(),
        "restore generated C# project:\n{}",
        build_output(&output)
    );
}

fn rebuild_project_without_restore(project: &Path) -> Output {
    Command::new("dotnet")
        .args([
            "build",
            "--no-restore",
            "--no-incremental",
            "--nologo",
            "--verbosity",
            "quiet",
        ])
        .current_dir(project)
        .output()
        .expect("run isolated dotnet build")
}

fn build_output(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn read_manifest(nef_path: &Path) -> ContractManifest {
    let manifest_path = nef_path.with_extension("manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path).unwrap_or_else(|error| {
        panic!(
            "read companion manifest {}: {error}",
            manifest_path.display()
        )
    });
    ContractManifest::from_json_str(&manifest_text).unwrap_or_else(|error| {
        panic!(
            "parse companion manifest {}: {error}",
            manifest_path.display()
        )
    })
}

fn decompile_csharp(nef_path: &Path) -> String {
    let bytes = fs::read(nef_path).expect("read NEF");
    let manifest = read_manifest(nef_path);
    Decompiler::new()
        .with_trace_comments(false)
        .with_typed_declarations(true)
        .decompile_bytes_with_manifest(&bytes, Some(manifest), OutputFormat::All)
        .expect("decompile NEF")
        .csharp
        .expect("C# output")
}

#[derive(Default)]
struct ContractDiagnostics {
    codes: BTreeMap<String, usize>,
    first: Option<String>,
}

fn sarif_diagnostics(project: &Path) -> ContractDiagnostics {
    let error_log = project.join("diagnostics.sarif");
    if !error_log.is_file() {
        return ContractDiagnostics::default();
    }
    let text = fs::read_to_string(&error_log).expect("read Roslyn SARIF error log");
    let sarif: serde_json::Value =
        serde_json::from_str(&text).expect("parse Roslyn SARIF error log");
    let mut diagnostics = ContractDiagnostics::default();
    for result in sarif
        .get("runs")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|run| run.get("results").and_then(serde_json::Value::as_array))
        .flatten()
    {
        if result.get("level").and_then(serde_json::Value::as_str) != Some("error") {
            continue;
        }
        if let Some(code) = result.get("ruleId").and_then(serde_json::Value::as_str) {
            *diagnostics.codes.entry(code.to_string()).or_default() += 1;
            if diagnostics.first.is_none() {
                let line = result
                    .pointer("/locations/0/physicalLocation/region/startLine")
                    .and_then(serde_json::Value::as_u64);
                let message = result
                    .pointer("/message/text")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Roslyn error");
                diagnostics.first = Some(line.map_or_else(
                    || format!("{code}: {message}"),
                    |line| format!("line {line}: {code}: {message}"),
                ));
            }
        }
    }
    diagnostics
}

fn output_diagnostics(output: &Output) -> ContractDiagnostics {
    let mut errors = BTreeSet::new();
    for line in build_output(output).lines() {
        if line.contains(": error ") {
            errors.insert(line.trim().to_string());
        }
    }
    let mut diagnostics = ContractDiagnostics::default();
    for error in errors {
        let Some((_, diagnostic)) = error.split_once(": error ") else {
            continue;
        };
        if let Some(code) = diagnostic
            .split(|character: char| character == ':' || character.is_ascii_whitespace())
            .find(|part| !part.is_empty())
        {
            *diagnostics.codes.entry(code.to_string()).or_default() += 1;
            diagnostics.first.get_or_insert(error);
        }
    }
    diagnostics
}

fn collect_nef_files(path: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(path).expect("read corpus directory") {
        let path = entry.expect("read corpus entry").path();
        if path.is_dir() {
            collect_nef_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "nef") {
            files.push(path);
        }
    }
}

#[test]
#[ignore = "requires dotnet and NEO_SMARTCONTRACT_FRAMEWORK_DLL"]
fn representative_generated_csharp_compiles_with_roslyn() {
    let (framework, target_framework) = framework_config();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cases = [
        "TestingArtifacts/edgecases/LoopIf.nef",
        "TestingArtifacts/edgecases/multi/MultiMethod.nef",
        "TestingArtifacts/edgecases/events/Events.nef",
        "TestingArtifacts/edgecases/permissions/Permissions.nef",
        "TestingArtifacts/edgecases/MethodToken.nef",
    ];

    for case in cases {
        let nef_path = root.join(case);
        let source = decompile_csharp(&nef_path);
        let project = tempdir().expect("create temporary project");
        let project_name = PathBuf::from(case)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("GeneratedContract")
            .replace('-', "_");
        write_project(project.path(), &framework, &target_framework);
        fs::write(project.path().join(format!("{project_name}.cs")), source)
            .expect("write generated C#");
        let output = build_project(project.path());
        assert!(
            output.status.success(),
            "Roslyn rejected generated C# for {case}:\n{}",
            build_output(&output)
        );
    }
}

#[test]
fn pinned_corpus_generated_csharp_compiles_with_roslyn() {
    let Some(corpus) = env::var_os("NEO_CSHARP_CORPUS_DIR").map(PathBuf::from) else {
        eprintln!("NEO_CSHARP_CORPUS_DIR is unset; skipping full C# corpus gate");
        return;
    };
    let (framework, target_framework) = framework_config();
    let provenance_text = fs::read_to_string(corpus.join("provenance.json"))
        .expect("full C# corpus gate requires provenance.json");
    let provenance: serde_json::Value =
        serde_json::from_str(&provenance_text).expect("parse corpus provenance.json");
    assert_eq!(
        provenance
            .pointer("/source/commit")
            .and_then(serde_json::Value::as_str),
        Some(PINNED_DEVPACK_COMMIT),
        "C# corpus is not the pinned neo-devpack-dotnet v3.10.0 revision"
    );

    let mut nef_files = Vec::new();
    collect_nef_files(&corpus, &mut nef_files);
    nef_files.sort();
    assert_eq!(nef_files.len(), 103, "pinned C# corpus count drift");
    for nef_path in &nef_files {
        let _ = read_manifest(nef_path);
    }

    let mut contract_names = BTreeSet::new();
    let generated_sources = nef_files
        .iter()
        .map(|nef_path| {
            let contract_name = nef_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .expect("UTF-8 corpus filename")
                .to_string();
            assert!(
                contract_names.insert(contract_name.clone()),
                "duplicate corpus contract name: {contract_name}"
            );
            (contract_name, decompile_csharp(nef_path))
        })
        .collect::<Vec<_>>();

    let project = tempdir().expect("create full-corpus C# project");
    write_project(project.path(), &framework, &target_framework);
    restore_project(project.path());

    let source_path = project.path().join("GeneratedContract.cs");
    let error_log = project.path().join("diagnostics.sarif");
    let mut contract_errors = BTreeMap::<String, ContractDiagnostics>::new();
    for (contract_name, source) in generated_sources {
        fs::write(&source_path, source).expect("write isolated generated C#");
        match fs::remove_file(&error_log) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("remove stale Roslyn SARIF error log: {error}"),
        }
        let output = rebuild_project_without_restore(project.path());
        if output.status.success() {
            continue;
        }
        let mut diagnostics = sarif_diagnostics(project.path());
        if diagnostics.codes.is_empty() {
            diagnostics = output_diagnostics(&output);
        }
        if diagnostics.codes.is_empty() {
            diagnostics.codes.insert("BUILD_FAILURE".to_string(), 1);
            diagnostics.first = Some("BUILD_FAILURE: dotnet build failed".to_string());
        }
        contract_errors.insert(contract_name, diagnostics);
    }

    let passed = nef_files.len() - contract_errors.len();
    let total_errors = contract_errors
        .values()
        .flat_map(|diagnostics| diagnostics.codes.values())
        .sum::<usize>();
    let mut error_codes = BTreeMap::<String, (usize, BTreeSet<String>)>::new();
    for (contract, diagnostics) in &contract_errors {
        for (code, count) in &diagnostics.codes {
            let aggregate = error_codes.entry(code.clone()).or_default();
            aggregate.0 += count;
            aggregate.1.insert(contract.clone());
        }
    }
    eprintln!(
        "C# compile census: {passed} passed, {} failed, {total_errors} errors",
        contract_errors.len()
    );
    if contract_errors.is_empty() {
        return;
    }

    eprintln!("Error codes (diagnostics / affected contracts):");
    for (code, (count, contracts)) in &error_codes {
        eprintln!("  {code}: {count} / {}", contracts.len());
    }
    eprintln!("Failing contracts (diagnostics by code):");
    for (contract, diagnostics) in &contract_errors {
        let summary = diagnostics
            .codes
            .iter()
            .map(|(code, count)| format!("{code}={count}"))
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!(
            "  {contract}: {summary}; first: {}",
            diagnostics.first.as_deref().unwrap_or("unavailable")
        );
    }
    panic!(
        "Roslyn rejected {} pinned generated C# contracts",
        contract_errors.len()
    );
}
