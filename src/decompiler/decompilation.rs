use crate::instruction::Instruction;
use crate::manifest::ContractManifest;
use crate::nef::NefFile;

use super::analysis::call_graph::CallGraph;
use super::analysis::types::TypeInfo;
use super::analysis::xrefs::Xrefs;
use super::cfg::Cfg;

/// Result of a successful decompilation run.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Decompilation {
    /// Parsed NEF container.
    pub nef: NefFile,
    /// Optional parsed contract manifest.
    pub manifest: Option<ContractManifest>,
    /// Non-fatal warnings emitted during disassembly or rendering.
    pub warnings: Vec<String>,
    /// Disassembled instruction stream from the NEF script.
    pub instructions: Vec<Instruction>,
    /// Control flow graph built from the instruction stream.
    pub cfg: Cfg,
    /// Best-effort call graph extracted from the instruction stream.
    pub call_graph: CallGraph,
    /// Best-effort cross-reference information for locals/args/statics.
    pub xrefs: Xrefs,
    /// Best-effort primitive/collection type inference.
    pub types: TypeInfo,
    /// Optional rendered pseudocode output.
    pub pseudocode: Option<String>,
    /// Optional rendered high-level output.
    pub high_level: Option<String>,
    /// Optional rendered C# output.
    pub csharp: Option<String>,
}

impl Decompilation {
    /// Get the control flow graph as DOT format for visualization.
    ///
    /// The DOT output can be rendered using Graphviz or similar tools.
    ///
    /// # Example
    /// ```ignore
    /// let decompilation = decompiler.decompile_bytes(&nef_bytes)?;
    /// let dot = decompilation.cfg_to_dot();
    /// std::fs::write("cfg.dot", dot)?;
    /// // Then run: dot -Tpng cfg.dot -o cfg.png
    /// ```
    #[must_use]
    pub fn cfg_to_dot(&self) -> String {
        self.cfg.to_dot()
    }
}
