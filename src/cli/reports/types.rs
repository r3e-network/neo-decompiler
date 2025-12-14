use serde::Serialize;

use super::instructions::InstructionReport;
use super::manifest::ManifestSummary;
use super::method_tokens::MethodTokenReport;
use crate::decompiler::analysis::{call_graph::CallGraph, types::TypeInfo, xrefs::Xrefs};

#[derive(Serialize)]
pub(in crate::cli) struct InfoReport {
    pub(in crate::cli) file: String,
    pub(in crate::cli) manifest_path: Option<String>,
    pub(in crate::cli) compiler: String,
    pub(in crate::cli) source: Option<String>,
    pub(in crate::cli) script_length: usize,
    pub(in crate::cli) script_hash_le: String,
    pub(in crate::cli) script_hash_be: String,
    pub(in crate::cli) checksum: String,
    pub(in crate::cli) method_tokens: Vec<MethodTokenReport>,
    pub(in crate::cli) manifest: Option<ManifestSummary>,
    pub(in crate::cli) warnings: Vec<String>,
}

#[derive(Serialize)]
pub(in crate::cli) struct TokensReport {
    pub(in crate::cli) file: String,
    pub(in crate::cli) method_tokens: Vec<MethodTokenReport>,
    pub(in crate::cli) warnings: Vec<String>,
}

#[derive(Serialize)]
pub(in crate::cli) struct DisasmReport {
    pub(in crate::cli) file: String,
    pub(in crate::cli) instructions: Vec<InstructionReport>,
    pub(in crate::cli) warnings: Vec<String>,
}

#[derive(Serialize)]
pub(in crate::cli) struct DecompileReport {
    pub(in crate::cli) file: String,
    pub(in crate::cli) manifest_path: Option<String>,
    pub(in crate::cli) script_hash_le: String,
    pub(in crate::cli) script_hash_be: String,
    pub(in crate::cli) csharp: String,
    pub(in crate::cli) high_level: String,
    pub(in crate::cli) pseudocode: String,
    pub(in crate::cli) instructions: Vec<InstructionReport>,
    pub(in crate::cli) method_tokens: Vec<MethodTokenReport>,
    pub(in crate::cli) manifest: Option<ManifestSummary>,
    pub(in crate::cli) analysis: AnalysisReport,
    pub(in crate::cli) warnings: Vec<String>,
}

#[derive(Serialize)]
pub(in crate::cli) struct AnalysisReport {
    pub(in crate::cli) call_graph: CallGraph,
    pub(in crate::cli) xrefs: Xrefs,
    pub(in crate::cli) types: TypeInfo,
}
