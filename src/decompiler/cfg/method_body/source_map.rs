use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::cfg::ssa::SsaForm;
use crate::decompiler::ir::{Block, ControlFlow, Stmt};
use crate::instruction::Instruction;

use super::{SourceMap, StatementId};

pub(super) fn build_source_map(
    body: &Block,
    ssa: &SsaForm,
    source_names: &BTreeMap<String, String>,
    instructions: &[Instruction],
) -> SourceMap {
    let all_offsets = instructions
        .iter()
        .map(|instruction| instruction.offset)
        .collect();
    let mut variable_origins = BTreeMap::<String, BTreeSet<usize>>::new();
    for (variable, block) in &ssa.definitions {
        let Some(cfg_block) = ssa.cfg.block(*block) else {
            continue;
        };
        let origins = cfg_block
            .instruction_range
            .clone()
            .filter_map(|index| {
                instructions
                    .get(index)
                    .map(|instruction| instruction.offset)
            })
            .collect::<BTreeSet<_>>();
        let raw_name = format!("{}_{}", variable.base, variable.version);
        let emitted = source_names
            .get(&variable.base)
            .or_else(|| source_names.get(&raw_name))
            .cloned()
            .unwrap_or(raw_name);
        variable_origins.entry(emitted).or_default().extend(origins);
    }

    let mut origins = BTreeMap::new();
    let mut next_id = 0u32;
    collect_source_origins(
        body,
        &variable_origins,
        &all_offsets,
        &mut next_id,
        &mut origins,
    );
    SourceMap {
        statement_origins: origins,
    }
}

fn collect_source_origins(
    block: &Block,
    variable_origins: &BTreeMap<String, BTreeSet<usize>>,
    all_offsets: &BTreeSet<usize>,
    next_id: &mut u32,
    origins: &mut BTreeMap<StatementId, BTreeSet<usize>>,
) {
    for statement in &block.stmts {
        let id = StatementId(*next_id);
        *next_id += 1;
        let mut names = BTreeSet::new();
        super::collect_statement_names(statement, &mut names);
        let mut statement_origins = BTreeSet::new();
        for name in names {
            if let Some(source) = variable_origins.get(&name) {
                statement_origins.extend(source.iter().copied());
            }
        }
        if statement_origins.is_empty() {
            statement_origins = all_offsets.clone();
        }
        origins.insert(id, statement_origins);
        collect_nested_source_origins(statement, variable_origins, all_offsets, next_id, origins);
    }
}

fn collect_nested_source_origins(
    statement: &Stmt,
    variable_origins: &BTreeMap<String, BTreeSet<usize>>,
    all_offsets: &BTreeSet<usize>,
    next_id: &mut u32,
    origins: &mut BTreeMap<StatementId, BTreeSet<usize>>,
) {
    let Stmt::ControlFlow(control) = statement else {
        return;
    };
    match control.as_ref() {
        ControlFlow::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_source_origins(then_branch, variable_origins, all_offsets, next_id, origins);
            if let Some(branch) = else_branch {
                collect_source_origins(branch, variable_origins, all_offsets, next_id, origins);
            }
        }
        ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
            collect_source_origins(body, variable_origins, all_offsets, next_id, origins);
        }
        ControlFlow::For { init, body, .. } => {
            if let Some(init) = init {
                collect_nested_source_origins(
                    init,
                    variable_origins,
                    all_offsets,
                    next_id,
                    origins,
                );
            }
            collect_source_origins(body, variable_origins, all_offsets, next_id, origins);
        }
        ControlFlow::TryCatch {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            collect_source_origins(try_body, variable_origins, all_offsets, next_id, origins);
            if let Some(body) = catch_body {
                collect_source_origins(body, variable_origins, all_offsets, next_id, origins);
            }
            if let Some(body) = finally_body {
                collect_source_origins(body, variable_origins, all_offsets, next_id, origins);
            }
        }
        ControlFlow::Switch { cases, default, .. } => {
            for (_, body) in cases {
                collect_source_origins(body, variable_origins, all_offsets, next_id, origins);
            }
            if let Some(body) = default {
                collect_source_origins(body, variable_origins, all_offsets, next_id, origins);
            }
        }
    }
}
