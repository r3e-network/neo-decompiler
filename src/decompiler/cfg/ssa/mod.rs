//! Static Single Assignment (SSA) construction and analysis.
//!
//! This module provides SSA transformation for Neo VM bytecode analysis.
//! SSA ensures each variable is assigned exactly once, enabling powerful
//! data flow analyses and optimizations.

pub mod builder;
mod dominance;
mod form;
mod variable;

pub use builder::{build_ssa_from_cfg, SsaBuilder};
pub use dominance::{compute, DominanceInfo};
pub use form::{SsaBlock, SsaExpr, SsaForm, SsaStats, SsaStmt, UseSite};
pub use variable::{PhiNode, SsaVariable};

use crate::decompiler::cfg::Cfg;

/// SSA conversion trait for extending CFG with SSA capabilities.
pub trait SsaConversion {
    /// Convert this CFG to SSA form.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_decompiler::decompiler::cfg::Cfg;
    ///
    /// let cfg = /* ... */;
    /// let ssa = cfg.to_ssa();
    /// ```
    fn to_ssa(&self) -> SsaForm;

    /// Compute dominance information for this CFG.
    ///
    /// Dominance information includes immediate dominators, dominator tree,
    /// and dominance frontiers needed for SSA construction.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_decompiler::decompiler::cfg::Cfg;
    ///
    /// let cfg = /* ... */;
    /// let dominance = cfg.compute_dominance();
    /// ```
    fn compute_dominance(&self) -> DominanceInfo;
}

impl SsaConversion for Cfg {
    fn to_ssa(&self) -> SsaForm {
        build_ssa_from_cfg(self)
    }

    fn compute_dominance(&self) -> DominanceInfo {
        compute(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssa_conversion_trait() {
        let cfg = Cfg::new();
        let ssa = cfg.to_ssa();

        // Should produce valid SSA form even for empty CFG
        assert_eq!(ssa.block_count(), 0);
    }

    #[test]
    fn test_compute_dominance_via_trait() {
        let cfg = Cfg::new();
        let dominance = cfg.compute_dominance();

        // Empty CFG has no dominance info
        assert!(dominance.idom.is_empty());
    }
}
