use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::cfg::ssa::{ssa_var_name, SsaForm, SsaVariable};
use crate::decompiler::cfg::BlockId;
use crate::decompiler::ir::{Expr, Literal, Stmt};

const VIRTUAL_ENTRY: BlockId = BlockId(usize::MAX);

#[derive(Clone, Debug, PartialEq, Eq)]
struct Copy {
    target: String,
    source: CopySource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CopySource {
    Variable(String),
    Null,
}

impl CopySource {
    fn from_ssa(variable: &SsaVariable, source_names: &BTreeMap<String, String>) -> Self {
        if variable.is_vm_null() {
            Self::Null
        } else {
            Self::Variable(ssa_var_name(variable, source_names))
        }
    }

    fn as_variable(&self) -> Option<&str> {
        match self {
            Self::Variable(name) => Some(name),
            Self::Null => None,
        }
    }

    fn into_expr(self) -> Expr {
        match self {
            Self::Variable(name) => Expr::Variable(name),
            Self::Null => Expr::Literal(Literal::Null),
        }
    }
}

pub(super) struct PhiLowering {
    edges: BTreeMap<(BlockId, BlockId), Vec<Copy>>,
    entries: BTreeMap<BlockId, Vec<Copy>>,
    used_names: BTreeSet<String>,
}

impl PhiLowering {
    pub(super) fn new(ssa: &SsaForm, source_names: &BTreeMap<String, String>) -> Self {
        let mut edges: BTreeMap<(BlockId, BlockId), Vec<Copy>> = BTreeMap::new();
        let mut entries: BTreeMap<BlockId, Vec<Copy>> = BTreeMap::new();
        let mut used_names: BTreeSet<String> = source_names.values().cloned().collect();
        let unknown = SsaVariable::new("?".to_string(), 0);

        for variable in ssa.definitions.keys().chain(ssa.uses.keys()) {
            used_names.insert(ssa_var_name(variable, source_names));
        }

        for block in ssa.blocks.values() {
            for phi in &block.phi_nodes {
                used_names.insert(ssa_var_name(&phi.target, source_names));
                for operand in phi.operands.values() {
                    used_names.insert(ssa_var_name(operand, source_names));
                }
            }
        }

        for (successor, block) in ssa.blocks_iter() {
            let predecessors: BTreeSet<_> = ssa
                .cfg
                .predecessors(*successor)
                .iter()
                .copied()
                .filter(|predecessor| {
                    *predecessor != VIRTUAL_ENTRY
                        && ssa.cfg.edge_kind(*predecessor, *successor)
                            != Some(crate::decompiler::cfg::EdgeKind::FinallyException)
                })
                .collect();

            for phi in &block.phi_nodes {
                if ssa.uses.get(&phi.target).is_none_or(BTreeSet::is_empty) {
                    continue;
                }

                let target = ssa_var_name(&phi.target, source_names);
                for predecessor in &predecessors {
                    let operand = phi.operands.get(predecessor).unwrap_or(&unknown);
                    edges
                        .entry((*predecessor, *successor))
                        .or_default()
                        .push(Copy {
                            target: target.clone(),
                            source: CopySource::from_ssa(operand, source_names),
                        });
                }

                if let Some(operand) = phi.operands.get(&VIRTUAL_ENTRY) {
                    entries.entry(*successor).or_default().push(Copy {
                        target,
                        source: CopySource::from_ssa(operand, source_names),
                    });
                }
            }
        }

        Self {
            edges,
            entries,
            used_names,
        }
    }

    pub(super) fn edge_statements(&self, from: BlockId, to: BlockId) -> Vec<Stmt> {
        self.schedule(
            self.edges
                .get(&(from, to))
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            &format!("{}_{}", from.index(), to.index()),
        )
    }

    pub(super) fn entry_statements(&self, entry: BlockId) -> Vec<Stmt> {
        self.schedule(
            self.entries.get(&entry).map(Vec::as_slice).unwrap_or(&[]),
            &format!("entry_{}", entry.index()),
        )
    }

    pub(super) fn fresh_name(&self, stem: &str) -> String {
        let mut suffix = 0usize;
        loop {
            let candidate = format!("_{stem}_{suffix}");
            if !self.used_names.contains(&candidate) {
                return candidate;
            }
            suffix += 1;
        }
    }

    fn schedule(&self, copies: &[Copy], scope: &str) -> Vec<Stmt> {
        let mut pending: Vec<Copy> = copies
            .iter()
            .filter(|copy| copy.source.as_variable() != Some(copy.target.as_str()))
            .cloned()
            .collect();
        let mut statements = Vec::new();
        let mut generated_names = BTreeSet::new();
        let mut next_temporary = 0usize;

        while !pending.is_empty() {
            let remaining_sources: BTreeSet<_> = pending
                .iter()
                .filter_map(|copy| copy.source.as_variable())
                .collect();
            if let Some(index) = pending
                .iter()
                .position(|copy| !remaining_sources.contains(copy.target.as_str()))
            {
                let copy = pending.remove(index);
                statements.push(Stmt::Assign {
                    target: copy.target,
                    value: copy.source.into_expr(),
                });
                continue;
            }

            let saved_source = pending
                .iter()
                .find_map(|copy| copy.source.as_variable())
                .expect("a parallel-copy cycle must contain a variable source")
                .to_string();
            let temporary = loop {
                let candidate = format!("_copy_tmp_{scope}_{next_temporary}");
                next_temporary += 1;
                let pending_uses_name = pending.iter().any(|copy| {
                    copy.target == candidate || copy.source.as_variable() == Some(&candidate)
                });
                if !self.used_names.contains(&candidate)
                    && !generated_names.contains(&candidate)
                    && !pending_uses_name
                {
                    break candidate;
                }
            };

            statements.push(Stmt::Assign {
                target: temporary.clone(),
                value: Expr::Variable(saved_source.clone()),
            });
            for copy in &mut pending {
                if copy.source.as_variable() == Some(saved_source.as_str()) {
                    copy.source = CopySource::Variable(temporary.clone());
                }
            }
            generated_names.insert(temporary);
        }

        statements
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::decompiler::cfg::ssa::{
        DominanceInfo, PhiNode, SsaBlock, SsaForm, SsaVariable, UseSite,
    };
    use crate::decompiler::cfg::{BlockId, Cfg, EdgeKind};
    use crate::decompiler::ir::{Expr, Stmt};

    fn variable(base: &str, version: usize) -> SsaVariable {
        SsaVariable::new(base.to_string(), version)
    }

    fn phi(target: SsaVariable, operands: &[(BlockId, SsaVariable)]) -> PhiNode {
        let mut phi = PhiNode::new(target);
        for (predecessor, operand) in operands {
            phi.add_operand(*predecessor, operand.clone());
        }
        phi
    }

    fn ssa_with_phis(
        edges: &[(BlockId, BlockId)],
        successor: BlockId,
        phis: Vec<PhiNode>,
        live_targets: &[SsaVariable],
    ) -> SsaForm {
        let mut cfg = Cfg::new();
        for (from, to) in edges {
            cfg.add_edge(*from, *to, EdgeKind::Unconditional);
        }

        let mut block = SsaBlock::new();
        for phi in phis {
            block.add_phi(phi);
        }

        let mut ssa = SsaForm::new(cfg, DominanceInfo::new());
        ssa.add_block(successor, block);
        for target in live_targets {
            ssa.add_use(target.clone(), UseSite::new(successor, 0));
        }
        ssa
    }

    fn source_names(names: &[(&str, &str)]) -> BTreeMap<String, String> {
        names
            .iter()
            .map(|(ssa_name, source_name)| (ssa_name.to_string(), source_name.to_string()))
            .collect()
    }

    fn assign(target: &str, source: &str) -> Stmt {
        Stmt::Assign {
            target: target.to_string(),
            value: Expr::Variable(source.to_string()),
        }
    }

    #[test]
    fn groups_live_phi_operands_by_incoming_edge() {
        let b1 = BlockId(1);
        let b2 = BlockId(2);
        let b3 = BlockId(3);
        let live = variable("live", 1);
        let dead = variable("dead", 1);
        let ssa = ssa_with_phis(
            &[(b1, b3), (b2, b3)],
            b3,
            vec![
                phi(
                    live.clone(),
                    &[(b1, variable("left", 1)), (b2, variable("right", 1))],
                ),
                phi(
                    dead,
                    &[
                        (b1, variable("stale_left", 1)),
                        (b2, variable("stale_right", 1)),
                    ],
                ),
            ],
            &[live],
        );
        let lowering = PhiLowering::new(
            &ssa,
            &source_names(&[
                ("live", "merged"),
                ("left", "left_value"),
                ("right", "right_value"),
                ("dead", "dead_value"),
                ("stale_left", "stale_left_value"),
                ("stale_right", "stale_right_value"),
            ]),
        );

        assert_eq!(
            lowering.edge_statements(b1, b3),
            vec![assign("merged", "left_value")]
        );
        assert_eq!(
            lowering.edge_statements(b2, b3),
            vec![assign("merged", "right_value")]
        );
    }

    #[test]
    fn de_versioned_slot_phi_does_not_emit_identity_copies() {
        let left = BlockId(1);
        let right = BlockId(2);
        let merge = BlockId(3);
        let target = variable("loc0", 2);
        let ssa = ssa_with_phis(
            &[(left, merge), (right, merge)],
            merge,
            vec![phi(
                target.clone(),
                &[(left, variable("loc0", 0)), (right, variable("loc0", 1))],
            )],
            &[target],
        );
        let lowering = PhiLowering::new(&ssa, &source_names(&[("loc0", "loc0")]));

        assert!(lowering.edge_statements(left, merge).is_empty());
        assert!(lowering.edge_statements(right, merge).is_empty());
    }

    #[test]
    fn fills_missing_real_predecessor_with_unknown() {
        let b1 = BlockId(1);
        let b2 = BlockId(2);
        let b3 = BlockId(3);
        let merged = variable("merged", 1);
        let ssa = ssa_with_phis(
            &[(b1, b3), (b2, b3)],
            b3,
            vec![phi(merged.clone(), &[(b1, variable("present", 1))])],
            &[merged],
        );
        let lowering = PhiLowering::new(
            &ssa,
            &source_names(&[("merged", "result"), ("present", "value")]),
        );

        assert_eq!(
            lowering.edge_statements(b1, b3),
            vec![assign("result", "value")]
        );
        assert_eq!(
            lowering.edge_statements(b2, b3),
            vec![assign("result", "?")]
        );
    }

    #[test]
    fn separates_virtual_entry_from_real_backedge() {
        let entry = BlockId(0);
        let backedge = BlockId(1);
        let merged = variable("merged", 1);
        let ssa = ssa_with_phis(
            &[(backedge, entry)],
            entry,
            vec![phi(
                merged.clone(),
                &[
                    (VIRTUAL_ENTRY, variable("initial", 1)),
                    (backedge, variable("next", 1)),
                ],
            )],
            &[merged],
        );
        let lowering = PhiLowering::new(
            &ssa,
            &source_names(&[
                ("merged", "state"),
                ("initial", "initial_state"),
                ("next", "next_state"),
            ]),
        );

        assert_eq!(
            lowering.entry_statements(entry),
            vec![assign("state", "initial_state")]
        );
        assert_eq!(
            lowering.edge_statements(backedge, entry),
            vec![assign("state", "next_state")]
        );
        assert_eq!(
            lowering.edge_statements(VIRTUAL_ENTRY, entry),
            Vec::<Stmt>::new()
        );
    }

    #[test]
    fn lowers_vm_null_phi_operands_to_literal_assignments() {
        let left = BlockId(1);
        let right = BlockId(2);
        let merge = BlockId(3);
        let target = variable("loc0", 1);
        let ssa = ssa_with_phis(
            &[(left, merge), (right, merge)],
            merge,
            vec![phi(
                target.clone(),
                &[(left, SsaVariable::vm_null()), (right, variable("loc0", 0))],
            )],
            &[target],
        );
        let lowering = PhiLowering::new(&ssa, &source_names(&[("loc0", "value")]));

        assert_eq!(
            lowering.edge_statements(left, merge),
            vec![Stmt::Assign {
                target: "value".to_string(),
                value: Expr::Literal(Literal::Null),
            }]
        );
        assert!(lowering.edge_statements(right, merge).is_empty());
    }

    #[test]
    fn schedules_acyclic_parallel_copies_without_clobbering_sources() {
        let from = BlockId(1);
        let to = BlockId(2);
        let a = variable("a_target", 1);
        let c = variable("c_target", 1);
        let ssa = ssa_with_phis(
            &[(from, to)],
            to,
            vec![
                phi(a.clone(), &[(from, variable("b_source", 1))]),
                phi(c.clone(), &[(from, variable("a_source", 1))]),
            ],
            &[a, c],
        );
        let lowering = PhiLowering::new(
            &ssa,
            &source_names(&[
                ("a_target", "a"),
                ("b_source", "b"),
                ("c_target", "c"),
                ("a_source", "a"),
            ]),
        );

        assert_eq!(
            lowering.edge_statements(from, to),
            vec![assign("c", "a"), assign("a", "b")]
        );
    }

    #[test]
    fn breaks_parallel_copy_cycle_with_one_unique_temporary() {
        let from = BlockId(1);
        let to = BlockId(2);
        let a = variable("a_target", 1);
        let b = variable("b_target", 1);
        let ssa = ssa_with_phis(
            &[(from, to)],
            to,
            vec![
                phi(a.clone(), &[(from, variable("b_source", 1))]),
                phi(b.clone(), &[(from, variable("a_source", 1))]),
            ],
            &[a, b],
        );
        let lowering = PhiLowering::new(
            &ssa,
            &source_names(&[
                ("a_target", "a"),
                ("b_source", "b"),
                ("b_target", "b"),
                ("a_source", "a"),
                ("reserved", "_copy_tmp_1_2_0"),
            ]),
        );

        assert_eq!(
            lowering.edge_statements(from, to),
            vec![
                assign("_copy_tmp_1_2_1", "b"),
                assign("b", "a"),
                assign("a", "_copy_tmp_1_2_1"),
            ]
        );
    }

    #[test]
    fn fresh_helper_name_avoids_lowered_source_names() {
        let ssa = ssa_with_phis(&[], BlockId(0), vec![], &[]);
        let lowering =
            PhiLowering::new(&ssa, &source_names(&[("reserved", "_do_while_first_0_0")]));

        assert_eq!(
            lowering.fresh_name("do_while_first_0"),
            "_do_while_first_0_1"
        );
    }
}
