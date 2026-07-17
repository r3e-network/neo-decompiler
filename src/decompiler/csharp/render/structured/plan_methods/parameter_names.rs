//! Source-meaningful parameter names for private helper methods.
//!
//! Inferred private helpers (`sub_0x...`) start out with `arg0/arg1/...`
//! placeholders. When every resolved call site passes the same
//! source-meaningful variable for a position — typically a manifest parameter
//! of the caller such as `amount`, possibly through simple wrappers like
//! `-amount` — the helper's parameter adopts that name. Votes that conflict
//! (different call sites passing different names) keep the placeholder.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::decompiler::cfg::method_body::{lower_method_body, Fidelity, MethodIrRequest};
use crate::decompiler::csharp::helpers::sanitize_csharp_identifier;
use crate::decompiler::ir::{Block, ControlFlow, Expr, SemanticCallTarget, Stmt};
use crate::instruction::Instruction;

use super::super::CSharpMethodPlan;

#[cfg(test)]
#[path = "parameter_names/tests.rs"]
mod tests;
#[path = "parameter_names/traversal.rs"]
mod traversal;

/// Rename placeholder parameters of inferred private helpers to the unanimous
/// source-level argument names observed at their call sites.
pub(super) fn infer_private_parameter_names(
    plans: &mut [CSharpMethodPlan],
    inferred_methods: &BTreeMap<usize, usize>,
    instructions: &[Instruction],
) {
    // target offset -> position -> vote accumulator
    let mut votes: BTreeMap<usize, BTreeMap<usize, NameVote>> = BTreeMap::new();
    // target offset -> call argument lists actually recovered
    let mut observed: BTreeMap<usize, usize> = BTreeMap::new();
    // target offset -> argument counts actually recovered at call sites
    let mut observed_arities: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    // Expected counts come from the resolved call contracts. A mismatch means
    // the faithful body and the call graph disagree, so no names are safe.
    let mut expected: BTreeMap<usize, usize> = BTreeMap::new();
    let mut expected_arities: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    // targets whose callers could not all be recovered faithfully
    let mut poisoned: HashSet<usize> = HashSet::new();

    for caller in plans.iter() {
        if caller.end <= caller.start {
            continue;
        }
        let targets = caller
            .method_context
            .calls_by_offset
            .values()
            .filter_map(|contract| match contract.target {
                SemanticCallTarget::Internal { offset, .. }
                    if inferred_methods.contains_key(&offset) =>
                {
                    Some(offset)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if targets.is_empty() {
            continue;
        }
        for contract in caller.method_context.calls_by_offset.values() {
            let SemanticCallTarget::Internal { offset, .. } = contract.target else {
                continue;
            };
            if !inferred_methods.contains_key(&offset) {
                continue;
            }
            *expected.entry(offset).or_default() += 1;
            expected_arities
                .entry(offset)
                .or_default()
                .insert(contract.argument_count);
        }

        let lowered = lower_method_body(MethodIrRequest {
            start: caller.start,
            end: caller.end,
            instructions,
            context: caller.method_context.clone(),
            symbol_types: caller.symbol_types.clone(),
            reduce_temps: false,
        });
        if lowered.fidelity.status == Fidelity::Incomplete {
            // Unrecovered call sites could cast missing votes; exclude these
            // targets from renaming entirely.
            poisoned.extend(targets);
            continue;
        }
        collect_name_hints(
            &lowered.body,
            &inferred_methods.keys().copied().collect(),
            &mut votes,
            &mut observed,
            &mut observed_arities,
        );
    }

    for (offset, plan_index) in inferred_methods {
        if poisoned.contains(offset) {
            continue;
        }
        if observed.get(offset).copied().unwrap_or(0) == 0 {
            continue;
        }
        let Some(expected_count) = expected.get(offset) else {
            continue;
        };
        let Some(expected_arities) = expected_arities.get(offset) else {
            continue;
        };
        if !recovered_call_shapes_match(
            *expected_count,
            expected_arities,
            observed.get(offset).copied().unwrap_or(0),
            observed_arities.get(offset),
        ) {
            continue;
        }
        let Some(position_votes) = votes.get(offset) else {
            continue;
        };
        let helper = &plans[*plan_index];
        let helper_symbols = lower_method_body(MethodIrRequest {
            start: helper.start,
            end: helper.end,
            instructions,
            context: helper.method_context.clone(),
            symbol_types: helper.symbol_types.clone(),
            reduce_temps: false,
        })
        .symbols;
        let local_names = helper_symbols
            .into_iter()
            .filter_map(|(name, symbol)| {
                matches!(
                    symbol.origin,
                    crate::decompiler::cfg::method_body::SymbolOrigin::Local(_)
                )
                .then_some(name)
            })
            .collect::<HashSet<_>>();
        let plan = &mut plans[*plan_index];
        let mut used: HashSet<String> = plan
            .parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect();
        used.extend(local_names);
        for (position, vote) in position_votes {
            let Some(name) = vote.unanimous() else {
                continue;
            };
            if *position >= plan.parameters.len()
                || !is_placeholder(&plan.parameters[*position].name)
            {
                continue;
            }
            let sanitized = sanitize_csharp_identifier(name);
            if is_placeholder(&sanitized) || used.contains(&sanitized) {
                continue;
            }
            used.insert(sanitized.clone());
            plan.parameters[*position].name = sanitized.clone();
            if let Some(argument_name) = plan.method_context.argument_names.get_mut(*position) {
                *argument_name = sanitized;
            }
        }
    }
}

fn recovered_call_shapes_match(
    expected_count: usize,
    expected_arities: &BTreeSet<usize>,
    observed_count: usize,
    observed_arities: Option<&BTreeSet<usize>>,
) -> bool {
    expected_arities.len() == 1
        && observed_count == expected_count
        && observed_arities == Some(expected_arities)
}

#[derive(Default)]
struct NameVote {
    name: Option<String>,
    conflicted: bool,
}

impl NameVote {
    fn offer(&mut self, hint: Option<String>) {
        match (self.name.clone(), hint) {
            (_, None) => self.conflicted = true,
            (None, Some(hint)) => self.name = Some(hint),
            (Some(existing), Some(hint)) if existing != hint => self.conflicted = true,
            _ => {}
        }
    }

    fn unanimous(&self) -> Option<&str> {
        (!self.conflicted).then_some(self.name.as_deref()).flatten()
    }
}

/// Placeholder families carry no source meaning: `arg3`, `loc2`, `t_7`,
/// `p4_0`, `static1`.
fn is_placeholder(name: &str) -> bool {
    for prefix in ["arg", "loc", "static"] {
        if let Some(suffix) = name.strip_prefix(prefix) {
            if !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()) {
                return true;
            }
        }
    }
    if let Some(suffix) = name.strip_prefix("t_") {
        if !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()) {
            return true;
        }
    }
    if let Some(rest) = name.strip_prefix('p') {
        if let Some((digits, version)) = rest.split_once('_') {
            if !digits.is_empty()
                && digits.bytes().all(|byte| byte.is_ascii_digit())
                && !version.is_empty()
                && version.bytes().all(|byte| byte.is_ascii_digit())
            {
                return true;
            }
        }
    }
    name == "?"
}

/// Extract a source-meaningful variable name from a call argument, looking
/// through wrappers that do not change identity (`-x`, `(T)x`, conversions).
fn name_hint(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Variable(name) if !is_placeholder(name) => Some(name.clone()),
        Expr::Variable(_) => None,
        Expr::Unary { operand, .. }
        | Expr::Cast { expr: operand, .. }
        | Expr::Convert { value: operand, .. }
        | Expr::IsType { value: operand, .. } => name_hint(operand),
        _ => None,
    }
}

fn collect_name_hints(
    block: &Block,
    targets: &HashSet<usize>,
    votes: &mut BTreeMap<usize, BTreeMap<usize, NameVote>>,
    observed: &mut BTreeMap<usize, usize>,
    observed_arities: &mut BTreeMap<usize, BTreeSet<usize>>,
) {
    // Single-definition copies let us see through SSA temporaries: when the
    // faithful body computes `t_21 = -amount; helper(@from, t_21)`, the
    // meaningful name for that argument still comes from `amount`.
    let mut definitions: BTreeMap<String, Vec<Expr>> = BTreeMap::new();
    collect_definitions(block, &mut definitions);

    for statement in &block.stmts {
        traversal::visit_statement_exprs(statement, &mut |expression| {
            let Expr::Call {
                target: SemanticCallTarget::Internal { offset, .. },
                args,
            } = expression
            else {
                return;
            };
            if !targets.contains(offset) {
                return;
            }
            *observed.entry(*offset).or_default() += 1;
            observed_arities
                .entry(*offset)
                .or_default()
                .insert(args.len());
            let position_votes = votes.entry(*offset).or_default();
            for (position, argument) in args.iter().enumerate() {
                let resolved = resolve_copy(argument, &definitions, 0);
                position_votes
                    .entry(position)
                    .or_default()
                    .offer(name_hint(resolved));
            }
        });
    }
}

fn collect_definitions(block: &Block, definitions: &mut BTreeMap<String, Vec<Expr>>) {
    for statement in &block.stmts {
        if let Stmt::Assign { target, value } = statement {
            definitions
                .entry(target.clone())
                .or_default()
                .push(value.clone());
        }
        if let Stmt::ControlFlow(control) = statement {
            match control.as_ref() {
                ControlFlow::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    collect_definitions(then_branch, definitions);
                    if let Some(branch) = else_branch {
                        collect_definitions(branch, definitions);
                    }
                }
                ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
                    collect_definitions(body, definitions);
                }
                ControlFlow::For { body, .. } => collect_definitions(body, definitions),
                ControlFlow::TryCatch {
                    try_body,
                    catch_body,
                    finally_body,
                    ..
                } => {
                    collect_definitions(try_body, definitions);
                    if let Some(body) = catch_body {
                        collect_definitions(body, definitions);
                    }
                    if let Some(body) = finally_body {
                        collect_definitions(body, definitions);
                    }
                }
                ControlFlow::Switch { cases, default, .. } => {
                    for (_, body) in cases {
                        collect_definitions(body, definitions);
                    }
                    if let Some(body) = default {
                        collect_definitions(body, definitions);
                    }
                }
            }
        }
    }
}

/// Follow placeholder variables to their single definition, if any.
fn resolve_copy<'a>(
    expression: &'a Expr,
    definitions: &'a BTreeMap<String, Vec<Expr>>,
    depth: usize,
) -> &'a Expr {
    if depth > 4 {
        return expression;
    }
    let Expr::Variable(name) = expression else {
        return expression;
    };
    if !is_placeholder(name) {
        return expression;
    }
    let Some([definition]) = definitions.get(name).map(Vec::as_slice) else {
        return expression;
    };
    resolve_copy(definition, definitions, depth + 1)
}
