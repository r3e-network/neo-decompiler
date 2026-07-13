use crate::decompiler::cfg::ssa::{DominanceInfo, SsaExpr, SsaForm, SsaStmt, SsaVariable};
use crate::decompiler::cfg::{BlockId, Cfg, Terminator};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
pub(super) fn compute_loop_headers(cfg: &Cfg, dominance: &DominanceInfo) -> HashSet<BlockId> {
    let mut headers = HashSet::new();
    for block in cfg.blocks() {
        for pred in cfg.predecessors(block.id) {
            if *pred == block.id || dominance.strictly_dominates(block.id, *pred) {
                headers.insert(block.id);
                break;
            }
        }
    }
    headers
}

pub(super) fn compute_postdominators(cfg: &Cfg) -> BTreeMap<BlockId, BTreeSet<BlockId>> {
    let blocks: BTreeSet<_> = cfg.blocks().map(|block| block.id).collect();
    let exits: BTreeSet<_> = blocks
        .iter()
        .copied()
        .filter(|block| cfg.successors(*block).is_empty())
        .collect();

    let mut reaches_exit = exits.clone();
    let mut queue: VecDeque<_> = exits.iter().copied().collect();
    while let Some(block) = queue.pop_front() {
        for predecessor in cfg.predecessors(block) {
            if reaches_exit.insert(*predecessor) {
                queue.push_back(*predecessor);
            }
        }
    }

    let mut postdominators: BTreeMap<_, _> = blocks
        .iter()
        .copied()
        .map(|block| {
            let initial = if exits.contains(&block) || !reaches_exit.contains(&block) {
                BTreeSet::from([block])
            } else {
                reaches_exit.clone()
            };
            (block, initial)
        })
        .collect();

    loop {
        let mut changed = false;
        for block in &blocks {
            if exits.contains(block) || !reaches_exit.contains(block) {
                continue;
            }
            let mut successors = cfg.successors(*block).iter();
            let Some(first) = successors.next() else {
                continue;
            };
            let mut next = postdominators
                .get(first)
                .cloned()
                .unwrap_or_else(|| BTreeSet::from([*first]));
            for successor in successors {
                let successor_postdominators = postdominators
                    .get(successor)
                    .cloned()
                    .unwrap_or_else(|| BTreeSet::from([*successor]));
                next.retain(|candidate| successor_postdominators.contains(candidate));
            }
            next.insert(*block);
            if postdominators.get(block) != Some(&next) {
                postdominators.insert(*block, next);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    postdominators
}

pub(super) fn find_irreducible_region(cfg: &Cfg) -> Option<BTreeSet<BlockId>> {
    for block in cfg.blocks() {
        let forward = reachable_cfg(cfg, block.id);
        let reverse = reverse_reachable_cfg(cfg, block.id);
        let region: BTreeSet<_> = forward.intersection(&reverse).copied().collect();
        if region.len() < 2 {
            continue;
        }
        let entry_targets = region
            .iter()
            .filter(|target| {
                cfg.predecessors(**target)
                    .iter()
                    .any(|predecessor| !region.contains(predecessor))
            })
            .count();
        if entry_targets >= 2 {
            return Some(region);
        }
    }
    None
}

pub(super) fn reachable_cfg(cfg: &Cfg, start: BlockId) -> BTreeSet<BlockId> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![start];
    while let Some(block) = stack.pop() {
        if !seen.insert(block) {
            continue;
        }
        stack.extend(cfg.successors(block));
    }
    seen
}

pub(super) fn reverse_reachable_cfg(cfg: &Cfg, start: BlockId) -> BTreeSet<BlockId> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![start];
    while let Some(block) = stack.pop() {
        if !seen.insert(block) {
            continue;
        }
        stack.extend(cfg.predecessors(block));
    }
    seen
}

pub(super) fn collect_structural_uses(ssa: &SsaForm) -> BTreeSet<SsaVariable> {
    let mut uses = BTreeSet::new();
    for block in ssa.blocks.values() {
        for phi in &block.phi_nodes {
            uses.extend(phi.operands.values().cloned());
        }
        for stmt in &block.stmts {
            match stmt {
                SsaStmt::Assign { value, .. }
                | SsaStmt::Expr(value)
                | SsaStmt::Return(Some(value))
                | SsaStmt::Throw(Some(value))
                | SsaStmt::Abort(Some(value)) => collect_expr_uses(value, &mut uses),
                SsaStmt::Assert { condition, message } => {
                    collect_expr_uses(condition, &mut uses);
                    if let Some(message) = message {
                        collect_expr_uses(message, &mut uses);
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
    uses
}

fn collect_expr_uses(expr: &SsaExpr, uses: &mut BTreeSet<SsaVariable>) {
    match expr {
        SsaExpr::Variable(variable) => {
            uses.insert(variable.clone());
        }
        SsaExpr::Binary { left, right, .. } => {
            collect_expr_uses(left, uses);
            collect_expr_uses(right, uses);
        }
        SsaExpr::Unary { operand, .. } => collect_expr_uses(operand, uses),
        SsaExpr::Call { args, .. } => args.iter().for_each(|arg| collect_expr_uses(arg, uses)),
        SsaExpr::Index { base, index } => {
            collect_expr_uses(base, uses);
            collect_expr_uses(index, uses);
        }
        SsaExpr::Member { base, .. } => collect_expr_uses(base, uses),
        SsaExpr::Cast { expr, .. } => collect_expr_uses(expr, uses),
        SsaExpr::Convert { value, .. } | SsaExpr::IsType { value, .. } => {
            collect_expr_uses(value, uses)
        }
        SsaExpr::NewArray { length, .. } => collect_expr_uses(length, uses),
        SsaExpr::Array(elements) | SsaExpr::Struct(elements) => {
            elements
                .iter()
                .for_each(|element| collect_expr_uses(element, uses));
        }
        SsaExpr::Map(pairs) => pairs.iter().for_each(|(key, value)| {
            collect_expr_uses(key, uses);
            collect_expr_uses(value, uses);
        }),
        SsaExpr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_uses(condition, uses);
            collect_expr_uses(then_expr, uses);
            collect_expr_uses(else_expr, uses);
        }
        SsaExpr::Literal(_) => {}
    }
}

pub(super) fn collect_leave_targets(cfg: &Cfg) -> BTreeSet<BlockId> {
    cfg.blocks()
        .filter_map(|block| match block.terminator {
            Terminator::EndTry {
                continuation,
                nonlocal: true,
            }
            | Terminator::EndTryFinally {
                continuation,
                nonlocal: true,
                ..
            } => Some(resolve_leave_target_cfg(cfg, continuation)),
            _ => None,
        })
        .collect()
}

pub(super) fn resolve_leave_target_cfg(cfg: &Cfg, mut target: BlockId) -> BlockId {
    let mut seen = BTreeSet::new();
    while seen.insert(target) {
        let Some(block) = cfg.block(target) else {
            break;
        };
        match block.terminator {
            Terminator::EndTry { continuation, .. }
            | Terminator::EndTryFinally { continuation, .. } => target = continuation,
            _ => break,
        }
    }
    target
}
