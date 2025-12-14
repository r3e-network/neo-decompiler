//! Program analysis helpers for lifted Neo N3 bytecode.
//!
//! The decompiler produces textual views (pseudocode/high-level/C#) as well as a
//! control-flow graph. This module adds best-effort analyses that are useful for
//! downstream tooling and future decompiler passes:
//!
//! - Call graph construction (internal calls, method tokens, syscalls)
//! - Slot cross-reference tracking (reads/writes for locals/args/statics)
//! - Lightweight type inference for common primitives and collections
//!
//! # Examples
//!
//! Inspecting the call graph and slot metadata for a contract:
//!
//! ```no_run
//! use neo_decompiler::{Decompiler, OutputFormat, Result};
//!
//! fn main() -> Result<()> {
//!     let decompiler = Decompiler::new();
//!     let decompilation = decompiler.decompile_file_with_manifest(
//!         "contract.nef",
//!         Some("contract.manifest.json"),
//!         OutputFormat::Pseudocode,
//!     )?;
//!
//!     for edge in &decompilation.call_graph.edges {
//!         println!(
//!             "0x{:04X}: {} -> {:?}",
//!             edge.call_offset, edge.caller.name, edge.target
//!         );
//!     }
//!
//!     let entry = &decompilation.xrefs.methods[0];
//!     println!("locals: {}", entry.locals.len());
//!     println!("arguments: {}", entry.arguments.len());
//!
//!     Ok(())
//! }
//! ```

mod methods;

pub mod call_graph;
pub mod types;
pub mod xrefs;

pub use methods::{MethodRef, MethodTable};
