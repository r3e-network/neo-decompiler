//! Neo N3 NEF inspection, disassembly, and decompilation tooling.
//!
//! This crate provides a well-tested toolkit for parsing NEF containers,
//! decoding Neo VM bytecode, building CFG/SSA views, and rendering both
//! high-level and C#-style outputs. It exposes the same core engine used by
//! the CLI binary in this repository, so library and command-line workflows
//! stay aligned.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

#[cfg(feature = "cli")]
pub mod cli;
pub mod decompiler;
pub mod disassembler;
pub mod error;
pub mod instruction;
pub mod manifest;
pub mod native_contracts;
pub mod nef;
pub mod syscalls;
mod util;
#[cfg(feature = "web")]
pub mod web;

pub use crate::decompiler::analysis::call_graph::{CallEdge, CallGraph, CallTarget};
pub use crate::decompiler::analysis::types::{MethodTypes, TypeInfo, ValueType};
pub use crate::decompiler::analysis::xrefs::{MethodXrefs, SlotKind, SlotXref, Xrefs};
pub use crate::decompiler::analysis::MethodRef;
pub use crate::decompiler::cfg::ssa::{
    DominanceInfo, PhiNode, SsaBlock, SsaConversion, SsaExpr, SsaForm, SsaStats, SsaStmt,
    SsaVariable,
};
pub use crate::decompiler::cfg::{
    BasicBlock, BlockId, Cfg, CfgBuilder, Edge, EdgeKind, Terminator,
};
pub use crate::decompiler::{Decompilation, Decompiler, OutputFormat};
pub use crate::disassembler::{Disassembler, UnknownHandling};
pub use crate::error::{Error, Result};
pub use crate::instruction::{Instruction, OpCode, Operand};
pub use crate::manifest::{ContractManifest, ManifestAbi, ManifestFeatures, ManifestMethod};
pub use crate::nef::{MethodToken, NefFile, NefHeader, NefParser};
