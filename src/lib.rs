//! Minimal Neo N3 NEF bytecode tooling.
//!
//! This crate provides a deliberately small and well tested toolkit for
//! inspecting Neo N3 NEF files.  It focuses on parsing the NEF container,
//! decoding a handful of common opcodes, and exposing a simple API that other
//! applications (including the CLI binary in this repository) can use.

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

pub use crate::decompiler::{Decompilation, Decompiler};
pub use crate::disassembler::{Disassembler, UnknownHandling};
pub use crate::error::{Error, Result};
pub use crate::instruction::{Instruction, OpCode, Operand};
pub use crate::manifest::{ContractManifest, ManifestAbi, ManifestFeatures, ManifestMethod};
pub use crate::nef::{MethodToken, NefFile, NefHeader, NefParser};
