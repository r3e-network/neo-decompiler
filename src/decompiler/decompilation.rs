use crate::instruction::Instruction;
use crate::manifest::ContractManifest;
use crate::nef::NefFile;

use super::analysis::call_graph::CallGraph;
use super::analysis::types::TypeInfo;
use super::analysis::xrefs::Xrefs;
use super::cfg::ssa::{SsaBuilder, SsaForm};
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
    /// SSA form of the control flow graph (computed lazily).
    pub ssa: Option<SsaForm>,
}

impl Decompilation {
    /// Get the control flow graph as DOT format for visualization.
    ///
    /// The DOT output can be rendered using Graphviz or similar tools.
    /// The graph carries a `label` attribute combining the contract
    /// name (when a manifest is provided), the script hash, and the
    /// instruction count, so a multi-CFG dump stays self-identifying.
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
        let title = self.cfg_dot_title();
        let mut dot = self.cfg.to_dot();
        // Splice the graph-level label between the `digraph CFG {`
        // header and the existing `node [shape=box];` line. Keeping
        // this layered above `cfg::to_dot` (rather than threading a
        // title argument through) means the lower-level graph
        // emitter remains agnostic of contract identity.
        if let Some(rest) = dot.strip_prefix("digraph CFG {\n") {
            let mut header = String::from("digraph CFG {\n");
            header.push_str(&format!("  label=\"{title}\";\n"));
            header.push_str("  labelloc=\"t\";\n");
            header.push_str(rest);
            dot = header;
        }
        dot
    }

    fn cfg_dot_title(&self) -> String {
        let script_hash = crate::util::format_hash(&self.nef.script_hash());
        let name = self
            .manifest
            .as_ref()
            .map(|m| m.name.trim())
            .filter(|n| !n.is_empty());
        let count = self.instructions.len();
        match name {
            Some(name) => format!("{name} ({script_hash}, {count} instr)"),
            None => format!("{script_hash} ({count} instr)"),
        }
    }

    /// Get the cached SSA form for this decompilation, if available.
    ///
    /// Call [`Self::compute_ssa`] first to populate the cached SSA value.
    ///
    /// # Returns
    ///
    /// `Option<&SsaForm>` - The SSA form, or `None` if CFG has no blocks.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let mut decompilation = decompiler.decompile_bytes(&nef_bytes)?;
    /// decompilation.compute_ssa();
    /// if let Some(ssa) = decompilation.ssa() {
    ///     println!("SSA Stats: {}", ssa.stats());
    ///     println!("{}", ssa.render());
    /// }
    /// ```
    #[must_use]
    pub fn ssa(&self) -> Option<&SsaForm> {
        self.ssa.as_ref()
    }

    /// Compute SSA form if not already computed.
    ///
    /// This is a convenience method for computing SSA form lazily.
    /// After calling this, `ssa()` will return `Some(...)`.
    pub fn compute_ssa(&mut self) {
        if self.ssa.is_none() && self.cfg.block_count() > 0 {
            // Use SsaBuilder with both instructions and CFG for full SSA construction
            let builder = SsaBuilder::new(&self.cfg, &self.instructions);
            self.ssa = Some(builder.build());
        }
    }

    /// Get SSA statistics if SSA form is available.
    ///
    /// # Returns
    ///
    /// `Option<String>` - Formatted statistics string, or `None` if SSA not computed.
    #[must_use]
    pub fn ssa_stats(&self) -> Option<String> {
        self.ssa.as_ref().map(|ssa| format!("{}", ssa.stats()))
    }

    /// Render SSA form if available.
    ///
    /// # Returns
    ///
    /// `Option<String>` - Rendered SSA code, or `None` if SSA not computed.
    #[must_use]
    pub fn render_ssa(&self) -> Option<String> {
        self.ssa.as_ref().map(SsaForm::render)
    }
}
