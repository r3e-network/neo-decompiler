//! Browser-friendly reports and optional WebAssembly bindings.

use crate::disassembler::UnknownHandling;
use crate::error::Result;
use crate::manifest::ContractManifest;
use crate::nef::NefParser;
use crate::{Decompiler, OutputFormat};

mod report;

pub use report::{WebDecompileReport, WebDisasmReport, WebInfoReport};

/// Options for browser-oriented disassembly.
#[derive(Debug, Clone, Copy, Default)]
pub struct WebDisasmOptions {
    /// Fail fast instead of emitting `UNKNOWN_0x..` instructions and warnings.
    pub fail_on_unknown_opcodes: bool,
}

/// Options for browser-oriented decompilation.
#[derive(Debug, Clone)]
pub struct WebDecompileOptions {
    /// Optional manifest JSON string to load alongside the NEF bytes.
    pub manifest_json: Option<String>,
    /// Enforce strict manifest validation when `manifest_json` is provided.
    pub strict_manifest: bool,
    /// Fail fast instead of emitting `UNKNOWN_0x..` instructions and warnings.
    pub fail_on_unknown_opcodes: bool,
    /// Enable the existing single-use temporary inlining pass.
    pub inline_single_use_temps: bool,
    /// Select which rendered outputs should be generated.
    pub output_format: OutputFormat,
}

impl Default for WebDecompileOptions {
    fn default() -> Self {
        Self {
            manifest_json: None,
            strict_manifest: false,
            fail_on_unknown_opcodes: false,
            inline_single_use_temps: false,
            output_format: OutputFormat::All,
        }
    }
}

/// Build a browser-friendly NEF info report from in-memory bytes.
///
/// The optional manifest input should be a UTF-8 JSON string.
///
/// # Errors
///
/// Returns an error if the NEF container or manifest is invalid.
pub fn info_report(nef_bytes: &[u8], manifest_json: Option<&str>) -> Result<WebInfoReport> {
    let nef = NefParser::new().parse(nef_bytes)?;
    let manifest = parse_manifest(manifest_json, false)?;
    Ok(report::build_info_report(&nef, manifest.as_ref()))
}

/// Build a browser-friendly disassembly report from in-memory bytes.
///
/// # Errors
///
/// Returns an error if the NEF container is invalid or disassembly fails.
pub fn disasm_report(nef_bytes: &[u8], options: WebDisasmOptions) -> Result<WebDisasmReport> {
    let handling = unknown_handling(options.fail_on_unknown_opcodes);
    let output = Decompiler::with_unknown_handling(handling).disassemble_bytes(nef_bytes)?;
    Ok(report::build_disasm_report(output))
}

/// Build a browser-friendly decompilation report from in-memory bytes.
///
/// # Errors
///
/// Returns an error if the NEF container or optional manifest is invalid, or
/// if disassembly/decompilation fails.
pub fn decompile_report(
    nef_bytes: &[u8],
    options: WebDecompileOptions,
) -> Result<WebDecompileReport> {
    let manifest = parse_manifest(options.manifest_json.as_deref(), options.strict_manifest)?;
    let handling = unknown_handling(options.fail_on_unknown_opcodes);
    let decompiler = Decompiler::with_unknown_handling(handling)
        .with_inline_single_use_temps(options.inline_single_use_temps);
    let result =
        decompiler.decompile_bytes_with_manifest(nef_bytes, manifest, options.output_format)?;
    Ok(report::build_decompile_report(result))
}

fn parse_manifest(manifest_json: Option<&str>, strict: bool) -> Result<Option<ContractManifest>> {
    manifest_json
        .map(|json| {
            if strict {
                ContractManifest::from_json_str_strict(json)
            } else {
                ContractManifest::from_json_str(json)
            }
        })
        .transpose()
}

fn unknown_handling(fail_on_unknown_opcodes: bool) -> UnknownHandling {
    if fail_on_unknown_opcodes {
        UnknownHandling::Error
    } else {
        UnknownHandling::Permit
    }
}

#[cfg(target_arch = "wasm32")]
use crate::Error;
#[cfg(target_arch = "wasm32")]
use serde::de::DeserializeOwned;
#[cfg(target_arch = "wasm32")]
use serde::Deserialize;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Default, Deserialize)]
struct JsInfoOptions {
    manifest_json: Option<String>,
    strict_manifest: bool,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Default, Deserialize)]
struct JsDisasmOptions {
    fail_on_unknown_opcodes: bool,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Default, Deserialize)]
struct JsDecompileOptions {
    manifest_json: Option<String>,
    strict_manifest: bool,
    fail_on_unknown_opcodes: bool,
    inline_single_use_temps: bool,
    output_format: Option<String>,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = initPanicHook)]
/// Install a panic hook that forwards Rust panics to the browser console.
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = infoReport)]
/// Build an info report from NEF bytes and a JS options object.
pub fn info_report_wasm(
    nef_bytes: &[u8],
    options: JsValue,
) -> std::result::Result<JsValue, JsValue> {
    let options: JsInfoOptions = parse_js_options(options)?;
    let manifest = parse_manifest(options.manifest_json.as_deref(), options.strict_manifest)
        .map_err(to_js_error)?;
    let nef = NefParser::new().parse(nef_bytes).map_err(to_js_error)?;
    serde_wasm_bindgen::to_value(&report::build_info_report(&nef, manifest.as_ref()))
        .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = disasmReport)]
/// Build a disassembly report from NEF bytes and a JS options object.
pub fn disasm_report_wasm(
    nef_bytes: &[u8],
    options: JsValue,
) -> std::result::Result<JsValue, JsValue> {
    let options: JsDisasmOptions = parse_js_options(options)?;
    let report = disasm_report(
        nef_bytes,
        WebDisasmOptions {
            fail_on_unknown_opcodes: options.fail_on_unknown_opcodes,
        },
    )
    .map_err(to_js_error)?;
    serde_wasm_bindgen::to_value(&report).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = decompileReport)]
/// Build a decompilation report from NEF bytes and a JS options object.
pub fn decompile_report_wasm(
    nef_bytes: &[u8],
    options: JsValue,
) -> std::result::Result<JsValue, JsValue> {
    let options: JsDecompileOptions = parse_js_options(options)?;
    let output_format = parse_output_format(options.output_format.as_deref())
        .map_err(|err| JsValue::from_str(&err))?;
    let report = decompile_report(
        nef_bytes,
        WebDecompileOptions {
            manifest_json: options.manifest_json,
            strict_manifest: options.strict_manifest,
            fail_on_unknown_opcodes: options.fail_on_unknown_opcodes,
            inline_single_use_temps: options.inline_single_use_temps,
            output_format,
        },
    )
    .map_err(to_js_error)?;
    serde_wasm_bindgen::to_value(&report).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[cfg(target_arch = "wasm32")]
fn parse_js_options<T>(value: JsValue) -> std::result::Result<T, JsValue>
where
    T: Default + DeserializeOwned,
{
    if value.is_null() || value.is_undefined() {
        Ok(T::default())
    } else {
        serde_wasm_bindgen::from_value(value)
            .map_err(|err| JsValue::from_str(&format!("invalid options: {err}")))
    }
}

#[cfg(target_arch = "wasm32")]
fn parse_output_format(value: Option<&str>) -> std::result::Result<OutputFormat, String> {
    match value.unwrap_or("all") {
        "all" => Ok(OutputFormat::All),
        "pseudocode" => Ok(OutputFormat::Pseudocode),
        "high_level" | "highLevel" => Ok(OutputFormat::HighLevel),
        "csharp" | "c_sharp" => Ok(OutputFormat::CSharp),
        other => Err(format!("invalid output_format: {other}")),
    }
}

#[cfg(target_arch = "wasm32")]
fn to_js_error(err: Error) -> JsValue {
    JsValue::from_str(&err.to_string())
}
