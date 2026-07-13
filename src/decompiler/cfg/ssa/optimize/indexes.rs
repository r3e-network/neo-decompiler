use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::cfg::BlockId;

use super::super::form::{SsaExpr, SsaForm, SsaStmt, UseSite};
use super::super::variable::SsaVariable;
use super::collect_expr_vars;

/// Recompute the `definitions` and `uses` indexes from the current blocks.
pub(super) fn rebuild_indexes(ssa: &mut SsaForm) {
    let terminator_uses: Vec<_> = ssa
        .uses
        .iter()
        .flat_map(|(variable, sites)| {
            sites
                .iter()
                .filter(|site| site.is_terminator())
                .cloned()
                .map(|site| (variable.clone(), site))
        })
        .collect();
    let mut definitions: BTreeMap<SsaVariable, BlockId> = BTreeMap::new();
    let mut uses: BTreeMap<SsaVariable, BTreeSet<UseSite>> = BTreeMap::new();
    for (bid, block) in ssa.blocks.iter() {
        for phi in &block.phi_nodes {
            definitions.insert(phi.target.clone(), *bid);
            for variable in phi.operands.values() {
                uses.entry(variable.clone())
                    .or_default()
                    .insert(UseSite::new(*bid, 0));
            }
        }
        for (index, statement) in block.stmts.iter().enumerate() {
            match statement {
                SsaStmt::Assign { target, value } => {
                    definitions.insert(target.clone(), *bid);
                    add_expr_uses(&mut uses, value, *bid, index);
                }
                SsaStmt::Return(Some(value))
                | SsaStmt::Throw(Some(value))
                | SsaStmt::Abort(Some(value))
                | SsaStmt::Expr(value) => add_expr_uses(&mut uses, value, *bid, index),
                SsaStmt::Assert { condition, message } => {
                    add_expr_uses(&mut uses, condition, *bid, index);
                    if let Some(message) = message {
                        add_expr_uses(&mut uses, message, *bid, index);
                    }
                }
                SsaStmt::Return(None)
                | SsaStmt::Throw(None)
                | SsaStmt::Abort(None)
                | SsaStmt::Phi(_)
                | SsaStmt::Other(_) => {}
            }
        }
    }
    for (variable, site) in terminator_uses {
        uses.entry(variable).or_default().insert(site);
    }
    ssa.definitions = definitions;
    ssa.uses = uses;
}

fn add_expr_uses(
    uses: &mut BTreeMap<SsaVariable, BTreeSet<UseSite>>,
    expression: &SsaExpr,
    block: BlockId,
    index: usize,
) {
    for variable in collect_expr_vars(expression) {
        uses.entry(variable)
            .or_default()
            .insert(UseSite::new(block, index));
    }
}
