//! Fuzz the public web report surface and the manifest-aware paths.
//!
//! The other targets feed bytes to the NEF parser or the library pipeline.
//! The browser-facing surface is a different entry point: it accepts a NEF
//! blob *and* an optional manifest JSON string, and it can run in strict
//! manifest mode. None of that combination is reachable from the other
//! targets, so a defect that only appears when a malformed manifest is
//! paired with a valid contract -- or when strict validation is enabled --
//! would go unnoticed.
//!
//! This target exercises:
//!
//! 1. `info_report` -- the NEF-only summary path.
//! 2. `disasm_report` -- disassembly with the unknown-opcode policy varied.
//! 3. `decompile_report` -- the full browser pipeline, with the manifest
//!    present or absent, strict or lenient, and the rendering options varied.
//! 4. `Decompiler::decompile_bytes_with_manifest` -- the library path that
//!    the web surface delegates to, reached with a manifest that may or may
//!    not agree with the contract.
//!
//! Two invariants are checked:
//!
//! 1. No panic.
//! 2. When a report is produced, its rendered C# stays brace-balanced, so a
//!    recovered control-flow shape never yields syntactically broken output.
#![no_main]

use libfuzzer_sys::fuzz_target;
use neo_decompiler::decompiler::ir::brace_balanced;
use neo_decompiler::manifest::ContractManifest;
use neo_decompiler::web::{disasm_report, info_report, WebDecompileOptions, WebDisasmOptions};
use neo_decompiler::{Decompiler, OutputFormat};

/// Reads decisions from the fuzzer input, cycling when it is exhausted.
struct Decisions<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Decisions<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn byte(&mut self) -> u8 {
        if self.data.is_empty() {
            return 0;
        }
        let value = self.data[self.pos % self.data.len()];
        self.pos += 1;
        value
    }

    fn bool(&mut self) -> bool {
        self.byte() & 1 == 0
    }
}

/// A minimal but structurally valid manifest JSON.
///
/// The ABI lists a method whose name and parameter count the decompiler uses
/// to match manifest entries against contract methods, so a manifest that
/// parses is enough to drive the manifest-aware code paths.
const MINIMAL_MANIFEST: &str = r#"{"name":"FuzzContract","groups":[],"features":{},"supportedstandards":[],"abi":{"methods":[{"name":"transfer","parameters":[{"name":"from","type":"Hash160"},{"name":"to","type":"Hash160"},{"name":"amount","type":"Integer"}],"returntype":"Boolean","offset":0,"parametersize":false}]}}"#;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    let mut decisions = Decisions::new(data);

    // The NEF blob is the raw input. It is often not a valid container, and
    // that is exactly the point: the report functions must reject it
    // cleanly rather than panic.
    let nef_bytes = data;

    // The manifest is either the fixed minimal document or a mutation of it,
    // so both a well-formed and a broken manifest are exercised.
    let manifest_json = if decisions.bool() {
        Some(MINIMAL_MANIFEST.to_string())
    } else {
        // Corrupt the manifest in a way that keeps it syntactically JSON-ish
        // most of the time but semantically wrong, so the strict validator
        // has something to reject.
        let mut corrupted = MINIMAL_MANIFEST.to_string();
        if let Some(index) = corrupted
            .char_indices()
            .nth(decisions.byte() as usize % corrupted.len())
            .map(|(i, _)| i)
        {
            corrupted.replace_range(index..index + 1, "x");
        }
        Some(corrupted)
    };

    // 1. The NEF-only summary path.
    let _ = info_report(nef_bytes, manifest_json.as_deref());

    // 2. Disassembly with the unknown-opcode policy varied.
    let disasm_options = WebDisasmOptions {
        fail_on_unknown_opcodes: decisions.bool(),
    };
    if let Ok(report) = disasm_report(nef_bytes, disasm_options) {
        let _ = report;
    }

    // 3. The full browser pipeline.
    let decompile_options = WebDecompileOptions {
        manifest_json: manifest_json.clone(),
        strict_manifest: decisions.bool(),
        fail_on_unknown_opcodes: decisions.bool(),
        inline_single_use_temps: decisions.bool(),
        emit_trace_comments: decisions.bool(),
        typed_declarations: decisions.bool(),
        high_level_from_ir: decisions.bool(),
        output_format: match decisions.byte() % 4 {
            0 => OutputFormat::All,
            1 => OutputFormat::Pseudocode,
            2 => OutputFormat::HighLevel,
            _ => OutputFormat::CSharp,
        },
    };

    if let Ok(report) = neo_decompiler::web::decompile_report(nef_bytes, decompile_options) {
        // The report fields are private because the browser surface reaches
        // them through serialization. Serializing here checks the same
        // payload the browser receives, including the rendered C#.
        let json = serde_json::to_string(&report).expect("the web report must serialize");
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) {
            if let Some(csharp) = value.get("csharp").and_then(|v| v.as_str()) {
                assert!(
                    brace_balanced(csharp),
                    "unbalanced braces in the web C# report:\n{csharp}"
                );
            }
        }
    }

    // 4. The library path the web surface delegates to, reached with a
    //    manifest that may or may not agree with the contract.
    let manifest = manifest_json
        .as_deref()
        .and_then(|json| ContractManifest::from_bytes(json.as_bytes()).ok());
    let decompiler = Decompiler::new();
    if let Ok(decompilation) =
        decompiler.decompile_bytes_with_manifest(nef_bytes, manifest, OutputFormat::All)
    {
        if let Some(csharp) = decompilation.csharp.as_deref() {
            assert!(
                brace_balanced(csharp),
                "unbalanced braces in the library C# output:\n{csharp}"
            );
        }
    }
});
