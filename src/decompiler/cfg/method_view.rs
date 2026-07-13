//! Per-method view of a contract for the structured-IR renderer.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::decompiler::analysis::call_graph::{CallGraph, CallTarget};
use crate::decompiler::analysis::method_contracts::{
    MethodContract, MethodContracts, ReturnBehavior,
};
use crate::decompiler::analysis::{MethodRef, MethodTable};
use crate::decompiler::cfg::ssa::{
    optimize_ssa, CallContract, MethodContext, SsaBuilder, SsaForm, SsaVariable,
};
use crate::decompiler::cfg::{structure_cfg_with_source_names, Cfg, CfgBuilder, Terminator};
use crate::decompiler::helpers::{
    build_method_labels_by_offset, format_manifest_parameters, format_manifest_type,
    make_unique_identifier, sanitize_identifier, sanitize_parameter_names,
};
use crate::decompiler::ir::render_block;
use crate::instruction::{Instruction, OpCode, Operand};
use crate::manifest::ContractManifest;

/// A per-method view: the method's instruction slice and a self-contained CFG
/// whose instruction ranges are local to that slice.
#[derive(Debug, Clone)]
pub(crate) struct MethodView {
    pub method: MethodRef,
    manifest_index: Option<usize>,
    pub cfg: Cfg,
    pub instructions: Vec<Instruction>,
}

/// Build an independent CFG from each method's instruction slice. Rebuilding
/// is necessary because whole-script blocks can straddle ABI method boundaries,
/// and their instruction ranges index the whole stream rather than the slice
/// consumed by the per-method SSA builder.
pub(crate) fn extract_method_cfgs(
    table: &MethodTable,
    instructions: &[Instruction],
) -> Vec<MethodView> {
    let mut out = Vec::new();
    for (start, end, method) in table.methods() {
        let method_instructions: Vec<_> = instructions
            .iter()
            .filter(|i| i.offset >= start && i.offset < end)
            .cloned()
            .collect();
        if method_instructions.is_empty() {
            continue;
        }
        let cfg = build_method_cfg(&method_instructions, start, end);
        out.push(MethodView {
            method: method.clone(),
            manifest_index: table.manifest_index_for_start(start),
            cfg,
            instructions: method_instructions,
        });
    }
    out
}

fn build_method_cfg(instructions: &[Instruction], start: usize, end: usize) -> Cfg {
    let built = CfgBuilder::new(instructions).build();
    let mut cfg = Cfg::new();

    for block in built.blocks() {
        let mut block = block.clone();
        if control_transfer_leaves_method(&block, instructions, start, end) {
            block.terminator = Terminator::Return;
        }
        cfg.add_block(block);
    }

    for edge in built.edges() {
        let retained = cfg
            .block(edge.from)
            .is_some_and(|block| block.terminator.successors().contains(&edge.to));
        if retained {
            cfg.add_edge(edge.from, edge.to, edge.kind);
        }
    }

    cfg
}

fn control_transfer_leaves_method(
    block: &crate::decompiler::cfg::BasicBlock,
    instructions: &[Instruction],
    start: usize,
    end: usize,
) -> bool {
    let Some(last_index) = block.instruction_range.end.checked_sub(1) else {
        return false;
    };
    let Some(instruction) = instructions.get(last_index) else {
        return false;
    };
    let is_conditional = matches!(
        instruction.opcode,
        OpCode::Jmpif
            | OpCode::Jmpif_L
            | OpCode::Jmpifnot
            | OpCode::Jmpifnot_L
            | OpCode::JmpEq
            | OpCode::JmpEq_L
            | OpCode::JmpNe
            | OpCode::JmpNe_L
            | OpCode::JmpGt
            | OpCode::JmpGt_L
            | OpCode::JmpGe
            | OpCode::JmpGe_L
            | OpCode::JmpLt
            | OpCode::JmpLt_L
            | OpCode::JmpLe
            | OpCode::JmpLe_L
    );
    let is_jump = is_conditional || matches!(instruction.opcode, OpCode::Jmp | OpCode::Jmp_L);
    if !is_jump {
        return false;
    }

    let target = match instruction.operand {
        Some(Operand::Jump(delta)) => instruction.offset.checked_add_signed(delta as isize),
        Some(Operand::Jump32(delta)) => instruction.offset.checked_add_signed(delta as isize),
        _ => None,
    };
    let target_leaves = target.is_some_and(|target| target < start || target >= end);
    let fallthrough_leaves = is_conditional && instructions.get(last_index + 1).is_none();
    target_leaves || fallthrough_leaves
}

/// Render a method body as `fn name(params) -> ret { body }`. Manifest metadata
/// is associated by the method table's ABI index, with an exact-offset fallback
/// for manually constructed views; unknown methods remain untyped and return void.
pub(crate) fn render_method_body(
    view: &MethodView,
    method_name: &str,
    manifest: Option<&ContractManifest>,
    calls_by_offset: &BTreeMap<usize, CallContract>,
    method_contract: Option<&MethodContract>,
) -> String {
    let manifest_method = manifest.and_then(|manifest| manifest_method_for_view(view, manifest));
    let (argument_names, returns_value) = match manifest_method {
        Some(method) => (
            sanitize_parameter_names(&method.parameters),
            Some(!method.return_type.eq_ignore_ascii_case("void")),
        ),
        None => method_contract.map_or_else(
            || (Vec::new(), None),
            |contract| {
                (
                    (0..contract.argument_count)
                        .map(|index| format!("arg{index}"))
                        .collect(),
                    Some(contract.return_behavior.returns_value()),
                )
            },
        ),
    };
    let context = MethodContext {
        argument_names,
        arguments_on_entry_stack: manifest_method.is_none()
            && method_contract.is_some()
            && view
                .instructions
                .first()
                .is_none_or(|instruction| instruction.opcode != OpCode::Initslot),
        returns_value,
        calls_by_offset: calls_for_view(view, calls_by_offset),
    };
    let mut ssa = SsaBuilder::new(&view.cfg, &view.instructions)
        .with_method_context(&context)
        .build();
    optimize_ssa(&mut ssa);
    let source_names = source_names_for_ssa(&context, &ssa);
    let block = structure_cfg_with_source_names(&ssa, &source_names);
    let body = render_block(&block, 0);
    let ret = manifest_method.map_or_else(
        || match method_contract.map(|contract| contract.return_behavior) {
            Some(ReturnBehavior::Void) => "void".to_string(),
            Some(ReturnBehavior::Value | ReturnBehavior::Unknown) | None => "any".to_string(),
        },
        |method| format_manifest_type(&method.return_type),
    );
    let parameters = manifest_method
        .map(|method| format_manifest_parameters(&method.parameters))
        .unwrap_or_else(|| {
            context
                .argument_names
                .iter()
                .map(|name| format!("{name}: any"))
                .collect::<Vec<_>>()
                .join(", ")
        });
    if body.trim().is_empty() {
        format!("    fn {method_name}({parameters}) -> {ret} {{\n        // empty body\n    }}\n")
    } else {
        let indented = body
            .lines()
            .map(|l| {
                if l.is_empty() {
                    String::new()
                } else {
                    format!("        {l}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("    fn {method_name}({parameters}) -> {ret} {{\n{indented}\n    }}\n")
    }
}

fn source_names_for_ssa(context: &MethodContext, ssa: &SsaForm) -> BTreeMap<String, String> {
    let mut display_names = context.source_names();
    let mut used: HashSet<String> = context.argument_names.iter().cloned().collect();
    let variables: BTreeSet<SsaVariable> = ssa
        .definitions
        .keys()
        .chain(ssa.uses.keys())
        .filter(|variable| !display_names.contains_key(&variable.base) && variable.base != "?")
        .cloned()
        .collect();

    for variable in variables {
        let generated = format!("{}_{}", variable.base, variable.version);
        let unique = make_unique_identifier(generated.clone(), &mut used);
        if unique != generated {
            display_names.insert(generated, unique);
        }
    }

    display_names
}

fn manifest_method_for_view<'a>(
    view: &MethodView,
    manifest: &'a ContractManifest,
) -> Option<&'a crate::manifest::ManifestMethod> {
    if let Some(index) = view.manifest_index {
        return manifest.abi.methods.get(index);
    }
    manifest.abi.methods.iter().find(|method| {
        method
            .offset
            .and_then(|offset| usize::try_from(offset).ok())
            == Some(view.method.offset)
    })
}

/// Compose the full contract view: legacy envelope header + per-method
/// bodies + closing `}`. Used by `Decompilation::render_structured_ir`.
pub(crate) fn render_envelope(
    nef: &crate::nef::NefFile,
    manifest: Option<&ContractManifest>,
    methods: &[MethodView],
    call_graph: &CallGraph,
    method_contracts: &MethodContracts,
) -> String {
    use crate::decompiler::write_contract_header;
    let mut instructions: Vec<_> = methods
        .iter()
        .flat_map(|view| view.instructions.iter().cloned())
        .collect();
    instructions.sort_by_key(|instruction| instruction.offset);
    let method_starts: Vec<_> = methods.iter().map(|view| view.method.offset).collect();
    let method_labels_by_offset = build_method_labels_by_offset(
        &instructions,
        &method_starts,
        manifest,
        sanitize_identifier,
        "script_entry",
    );
    let calls_by_offset =
        build_call_contracts(call_graph, method_contracts, &method_labels_by_offset);

    let mut out = String::new();
    write_contract_header(&mut out, nef, manifest);
    for view in methods {
        let method_name = method_labels_by_offset
            .get(&view.method.offset)
            .map_or_else(|| sanitize_identifier(&view.method.name), Clone::clone);
        out.push_str(&render_method_body(
            view,
            &method_name,
            manifest,
            &calls_by_offset,
            method_contracts.get(view.method.offset),
        ));
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

fn calls_for_view(
    view: &MethodView,
    calls_by_offset: &BTreeMap<usize, CallContract>,
) -> BTreeMap<usize, CallContract> {
    view.instructions
        .iter()
        .filter_map(|instruction| {
            calls_by_offset
                .get(&instruction.offset)
                .cloned()
                .map(|contract| (instruction.offset, contract))
        })
        .collect()
}

fn build_call_contracts(
    call_graph: &CallGraph,
    method_contracts: &MethodContracts,
    method_labels_by_offset: &BTreeMap<usize, String>,
) -> BTreeMap<usize, CallContract> {
    let mut contracts = BTreeMap::new();
    for edge in &call_graph.edges {
        let contract = match &edge.target {
            CallTarget::Internal { method } => {
                let method_contract = method_contracts.get(method.offset);
                CallContract::new(
                    method_labels_by_offset
                        .get(&method.offset)
                        .cloned()
                        .unwrap_or_else(|| sanitize_identifier(&method.name)),
                    method_contract.map_or(0, |contract| contract.argument_count),
                    method_contract.is_none_or(|contract| contract.return_behavior.returns_value()),
                )
            }
            CallTarget::MethodToken {
                method,
                parameters_count,
                has_return_value,
                ..
            } => CallContract::new(
                sanitize_identifier(method),
                usize::from(*parameters_count),
                *has_return_value,
            ),
            _ => continue,
        };
        contracts.insert(edge.call_offset, contract);
    }
    contracts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompiler::cfg::{BasicBlock, BlockId};
    use crate::instruction::{Instruction, OpCode, Operand};

    fn ins(offset: usize, op: OpCode) -> Instruction {
        Instruction::new(offset, op, None)
    }

    #[test]
    fn extract_builds_local_cfgs_and_rewrites_cross_range_jump() {
        let instructions = vec![
            Instruction::new(0, OpCode::Jmp, Some(Operand::Jump(10))),
            ins(10, OpCode::Push0),
            ins(11, OpCode::Ret),
        ];
        let manifest_json = r#"{"name":"C","abi":{"methods":[
            {"name":"main","parameters":[],"returntype":"Integer","offset":0},
            {"name":"helper","parameters":[],"returntype":"Integer","offset":10}
        ]}}"#;
        let manifest: crate::manifest::ContractManifest =
            serde_json::from_str(manifest_json).unwrap();
        let table = MethodTable::new(&instructions, Some(&manifest));
        let views = extract_method_cfgs(&table, &instructions);
        assert_eq!(views.len(), 2, "expected two method spans");
        let a = &views[0];
        let a0 = a.cfg.block(BlockId(0)).expect("block 0 in A");
        assert!(matches!(a0.terminator, Terminator::Return));
        assert_eq!(a0.instruction_range, 0..1);
        let b = &views[1];
        let b0 = b.cfg.block(BlockId(0)).expect("local block 0 in B");
        assert_eq!(b0.start_offset, 10);
        assert_eq!(b0.instruction_range, 0..2);
        assert!(matches!(b0.terminator, Terminator::Return));
    }

    #[test]
    fn render_method_body_emits_fn_with_return_type() {
        // A trivial method: PUSH1 ; RET → `fn main() -> Integer { ... return ... }`.
        let instructions = vec![
            Instruction::new(0, OpCode::Push1, None),
            Instruction::new(1, OpCode::Ret, None),
        ];
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(BlockId(0), 0, 2, 0..2, Terminator::Return));
        let view = MethodView {
            method: MethodRef {
                offset: 0,
                name: "main".to_string(),
            },
            manifest_index: Some(0),
            cfg,
            instructions,
        };
        let manifest_json = r#"{"name":"C","abi":{"methods":[
            {"name":"main","parameters":[],"returntype":"Integer"}
        ]}}"#;
        let manifest: ContractManifest = serde_json::from_str(manifest_json).unwrap();
        let out = render_method_body(&view, "main", Some(&manifest), &BTreeMap::new(), None);
        assert!(out.contains("fn main() -> int"), "got:\n{out}");
        assert!(out.contains("return"), "got:\n{out}");
    }

    #[test]
    fn render_void_method_does_not_return_ambient_value_across_call() {
        let instructions = vec![
            Instruction::new(0, OpCode::Push1, None),
            Instruction::new(1, OpCode::Call, Some(Operand::Jump(4))),
            Instruction::new(3, OpCode::Ret, None),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let view = MethodView {
            method: MethodRef {
                offset: 0,
                name: "main".to_string(),
            },
            manifest_index: None,
            cfg,
            instructions,
        };
        let manifest_json = r#"{"name":"C","abi":{"methods":[
            {"name":"main","parameters":[],"returntype":"Void","offset":0}
        ]}}"#;
        let manifest: ContractManifest = serde_json::from_str(manifest_json).unwrap();

        let out = render_method_body(&view, "main", Some(&manifest), &BTreeMap::new(), None);
        assert!(
            out.contains("call_0x0005()"),
            "call must remain visible:\n{out}"
        );
        assert!(
            out.contains("return;"),
            "void method must use bare return:\n{out}"
        );
        assert!(
            !out.contains("return 1;"),
            "ambient value must not escape:\n{out}"
        );
    }

    #[test]
    fn render_method_body_does_not_associate_manifest_by_name_only() {
        let instructions = vec![Instruction::new(6, OpCode::Ret, None)];
        let cfg = CfgBuilder::new(&instructions).build();
        let view = MethodView {
            method: MethodRef {
                offset: 6,
                name: "sub_0x0006".to_string(),
            },
            manifest_index: None,
            cfg,
            instructions,
        };
        let manifest_json = r#"{"name":"C","abi":{"methods":[
            {"name":"sub_0x0006","parameters":[{"name":"value","type":"Integer"}],
             "returntype":"Boolean","offset":10}
        ]}}"#;
        let manifest: ContractManifest = serde_json::from_str(manifest_json).unwrap();

        let out = render_method_body(&view, "sub_0x0006", Some(&manifest), &BTreeMap::new(), None);

        assert!(
            out.contains("fn sub_0x0006() -> any"),
            "a name match at another offset must not supply an ABI contract:\n{out}"
        );
        assert!(
            !out.contains("value: int") && !out.contains("-> bool"),
            "unassociated inferred method must remain untyped:\n{out}"
        );
    }
}
