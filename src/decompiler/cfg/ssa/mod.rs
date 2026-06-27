//! Static Single Assignment (SSA) construction and analysis.
//!
//! This module provides SSA transformation for Neo VM bytecode analysis.
//! SSA ensures each variable is assigned exactly once, enabling powerful
//! data flow analyses and optimizations.

pub mod builder;
mod dominance;
mod effects;
mod form;
mod optimize;
mod to_ir;
mod variable;

pub use builder::SsaBuilder;
pub use dominance::{compute, DominanceInfo};
pub use form::{SsaBlock, SsaExpr, SsaForm, SsaStats, SsaStmt, UseSite};
pub use optimize::optimize as optimize_ssa;
pub use to_ir::{render_ssa_form, ssa_expr_to_ir};
pub use variable::{PhiNode, SsaVariable};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompiler::cfg::Cfg;

    #[test]
    fn test_dominance_via_cfg() {
        let cfg = Cfg::new();
        let dominance = compute(&cfg);

        // Empty CFG has no dominance info
        assert!(dominance.idom.is_empty());
    }
}
