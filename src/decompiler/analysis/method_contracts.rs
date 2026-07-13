//! Shared stack-call contracts for manifest and inferred methods.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::decompiler::cfg::method_view::{extract_method_cfgs, MethodView};
use crate::decompiler::cfg::ssa::{CallContract, MethodContext, SsaBuilder, SsaStmt};
use crate::decompiler::helpers::{
    build_method_arg_counts_by_offset, build_method_returns_value_by_offset, sanitize_identifier,
};
use crate::instruction::{Instruction, OpCode};
use crate::manifest::ContractManifest;

use super::call_graph::{CallGraph, CallTarget};
use super::{MethodRef, MethodTable};

/// Whether a method is known to return a value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReturnBehavior {
    /// A manifest declaration guarantees that the method returns a value.
    Value,
    /// A manifest declaration or conservative inference proves a bare return.
    Void,
    /// No declaration or safe inference establishes the return behavior.
    #[default]
    Unknown,
}

impl ReturnBehavior {
    pub(crate) const fn returns_value(self) -> bool {
        !matches!(self, Self::Void)
    }
}

/// Stack-call metadata for one method in a script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MethodContract {
    /// Method identified by bytecode offset and stable display name.
    pub method: MethodRef,
    /// Number of values consumed from the evaluation stack by a call.
    pub argument_count: usize,
    /// Declared or inferred return behavior.
    pub return_behavior: ReturnBehavior,
}

/// Deterministic method-contract analysis for a script.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MethodContracts {
    /// Contracts sorted by method entry offset.
    pub methods: Vec<MethodContract>,
}

impl MethodContracts {
    /// Find the contract whose method begins at `offset`.
    #[must_use]
    pub fn get(&self, offset: usize) -> Option<&MethodContract> {
        self.methods
            .binary_search_by_key(&offset, |contract| contract.method.offset)
            .ok()
            .map(|index| &self.methods[index])
    }

    pub(crate) fn argument_counts_by_offset(&self) -> BTreeMap<usize, usize> {
        self.methods
            .iter()
            .map(|contract| (contract.method.offset, contract.argument_count))
            .collect()
    }

    pub(crate) fn returns_value_by_offset(&self) -> BTreeMap<usize, bool> {
        self.methods
            .iter()
            .map(|contract| {
                (
                    contract.method.offset,
                    contract.return_behavior.returns_value(),
                )
            })
            .collect()
    }
}

/// Infer shared stack-call contracts for every stable method in `call_graph`.
///
/// Manifest declarations are authoritative. Undeclared internal callees begin
/// as [`ReturnBehavior::Unknown`] and transition only to
/// [`ReturnBehavior::Void`] when SSA observes at least one return and every
/// observed return is bare. Unknown calls remain conservatively value-producing
/// while the fixed point is evaluated.
#[must_use]
pub fn infer_method_contracts(
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
    call_graph: &CallGraph,
) -> MethodContracts {
    let methods_by_offset: BTreeMap<usize, MethodRef> = call_graph
        .methods
        .iter()
        .cloned()
        .map(|method| (method.offset, method))
        .collect();
    let method_starts: Vec<_> = methods_by_offset.keys().copied().collect();
    let argument_counts = build_method_arg_counts_by_offset(instructions, &method_starts, manifest);
    let declared_returns = build_method_returns_value_by_offset(instructions, manifest);

    let mut contracts: BTreeMap<usize, MethodContract> = methods_by_offset
        .into_iter()
        .map(|(offset, method)| {
            let return_behavior =
                declared_returns
                    .get(&offset)
                    .map_or(ReturnBehavior::Unknown, |returns_value| {
                        if *returns_value {
                            ReturnBehavior::Value
                        } else {
                            ReturnBehavior::Void
                        }
                    });
            (
                offset,
                MethodContract {
                    method,
                    argument_count: argument_counts.get(&offset).copied().unwrap_or(0),
                    return_behavior,
                },
            )
        })
        .collect();

    let candidates: BTreeSet<_> = call_graph
        .edges
        .iter()
        .filter_map(|edge| match &edge.target {
            CallTarget::Internal { method }
                if !declared_returns.contains_key(&method.offset)
                    && contracts.contains_key(&method.offset) =>
            {
                Some(method.offset)
            }
            _ => None,
        })
        .collect();
    let table = MethodTable::new(instructions, manifest);
    let views = extract_method_cfgs(&table, instructions);
    let views_by_offset: BTreeMap<_, _> = views
        .iter()
        .map(|view| (view.method.offset, view))
        .collect();

    loop {
        let calls_by_offset = build_call_contracts(call_graph, &contracts);
        let newly_void: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|offset| {
                contracts
                    .get(offset)
                    .is_some_and(|contract| contract.return_behavior == ReturnBehavior::Unknown)
            })
            .filter(|offset| {
                let Some(view) = views_by_offset.get(offset).copied() else {
                    return false;
                };
                let argument_count = contracts
                    .get(offset)
                    .map_or(0, |contract| contract.argument_count);
                method_has_only_bare_returns(view, &calls_by_offset, argument_count)
            })
            .collect();

        if newly_void.is_empty() {
            break;
        }
        for offset in newly_void {
            if let Some(contract) = contracts.get_mut(&offset) {
                contract.return_behavior = ReturnBehavior::Void;
            }
        }
    }

    MethodContracts {
        methods: contracts.into_values().collect(),
    }
}

fn method_has_only_bare_returns(
    view: &MethodView,
    calls_by_offset: &BTreeMap<usize, CallContract>,
    argument_count: usize,
) -> bool {
    let context = MethodContext {
        argument_names: (0..argument_count)
            .map(|index| format!("arg{index}"))
            .collect(),
        arguments_on_entry_stack: view
            .instructions
            .first()
            .is_none_or(|instruction| instruction.opcode != OpCode::Initslot),
        calls_by_offset: calls_for_view(view, calls_by_offset),
        ..MethodContext::default()
    };
    let ssa = SsaBuilder::new(&view.cfg, &view.instructions)
        .with_method_context(&context)
        .build();
    let returns: Vec<_> = ssa
        .blocks_iter()
        .flat_map(|(_, block)| &block.stmts)
        .filter_map(|statement| match statement {
            SsaStmt::Return(value) => Some(value.is_none()),
            _ => None,
        })
        .collect();

    !returns.is_empty() && returns.iter().all(|is_bare| *is_bare)
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
    contracts: &BTreeMap<usize, MethodContract>,
) -> BTreeMap<usize, CallContract> {
    let mut calls = BTreeMap::new();
    for edge in &call_graph.edges {
        let contract = match &edge.target {
            CallTarget::Internal { method } => {
                let method_contract = contracts.get(&method.offset);
                CallContract::new(
                    method.name.clone(),
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
        calls.insert(edge.call_offset, contract);
    }
    calls
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::decompiler::analysis::call_graph::build_call_graph;
    use crate::decompiler::analysis::MethodRef;
    use crate::disassembler::Disassembler;
    use crate::manifest::ContractManifest;
    use crate::nef::{MethodToken, NefFile, NefHeader};

    use super::{infer_method_contracts, MethodContract, MethodContracts, ReturnBehavior};

    const PRIVATE_VOID_LEAF: &[u8] = &[
        0x19, 0x11, 0x34, 0x05, 0x40, 0x21, 0x21, 0x57, 0x00, 0x01, 0x78, 0x45, 0x40,
    ];

    fn manifest(json: &str) -> ContractManifest {
        ContractManifest::from_json_str(json).expect("manifest parses")
    }

    fn analyze(script: &[u8], manifest: Option<&ContractManifest>) -> MethodContracts {
        analyze_with_tokens(script, manifest, Vec::new())
    }

    fn analyze_with_tokens(
        script: &[u8],
        manifest: Option<&ContractManifest>,
        method_tokens: Vec<MethodToken>,
    ) -> MethodContracts {
        let instructions = Disassembler::new()
            .disassemble(script)
            .expect("script disassembles");
        let nef = NefFile {
            header: NefHeader {
                magic: *b"NEF3",
                compiler: "test".to_string(),
                source: String::new(),
            },
            method_tokens,
            script: script.to_vec(),
            checksum: 0,
        };
        let call_graph = build_call_graph(&nef, &instructions, manifest);
        infer_method_contracts(&instructions, manifest, &call_graph)
    }

    fn standard_manifest() -> ContractManifest {
        manifest(
            r#"{
                "name": "Contract",
                "abi": { "methods": [{
                    "name": "main", "parameters": [], "returntype": "Integer", "offset": 0
                }] }
            }"#,
        )
    }

    #[test]
    fn infers_private_void_leaf_with_entry_arity() {
        let manifest = standard_manifest();

        let contracts = analyze(PRIVATE_VOID_LEAF, Some(&manifest));
        let helper = contracts.get(7).expect("private helper contract");

        assert_eq!(helper.argument_count, 1);
        assert_eq!(helper.return_behavior, ReturnBehavior::Void);
    }

    #[test]
    fn infers_five_entry_arguments_for_private_memcpy_helper() {
        let manifest = standard_manifest();
        let script = [0x34, 0x04, 0x40, 0x21, 0x89, 0x40];

        let contracts = analyze(&script, Some(&manifest));
        let helper = contracts.get(4).expect("private MEMCPY helper contract");

        assert_eq!(helper.argument_count, 5);
        assert_eq!(helper.return_behavior, ReturnBehavior::Void);
    }

    #[test]
    fn converges_private_void_wrapper_chain_from_leaf_to_caller() {
        let manifest = standard_manifest();
        let script = [
            0x19, 0x34, 0x05, 0x40, 0x21, 0x21, 0x34, 0x04, 0x40, 0x21, 0x40,
        ];

        let contracts = analyze(&script, Some(&manifest));

        assert_eq!(
            contracts.get(6).map(|contract| contract.return_behavior),
            Some(ReturnBehavior::Void)
        );
        assert_eq!(
            contracts.get(10).map(|contract| contract.return_behavior),
            Some(ReturnBehavior::Void)
        );
    }

    #[test]
    fn keeps_recursive_private_method_unknown() {
        let manifest = standard_manifest();
        let script = [0x19, 0x34, 0x05, 0x40, 0x21, 0x21, 0x34, 0x00, 0x40];

        let contracts = analyze(&script, Some(&manifest));

        assert_eq!(
            contracts.get(6).map(|contract| contract.return_behavior),
            Some(ReturnBehavior::Unknown)
        );
    }

    #[test]
    fn keeps_mixed_return_private_method_unknown() {
        let manifest = standard_manifest();
        let script = [
            0x34, 0x06, 0x40, 0x21, 0x21, 0x21, 0x11, 0x26, 0x04, 0x11, 0x40, 0x40,
        ];

        let contracts = analyze(&script, Some(&manifest));

        assert_eq!(
            contracts.get(6).map(|contract| contract.return_behavior),
            Some(ReturnBehavior::Unknown)
        );
    }

    #[test]
    fn keeps_private_method_without_return_unknown() {
        let manifest = standard_manifest();
        let script = [0x34, 0x04, 0x40, 0x21, 0x38];

        let contracts = analyze(&script, Some(&manifest));

        assert_eq!(
            contracts.get(4).map(|contract| contract.return_behavior),
            Some(ReturnBehavior::Unknown)
        );
    }

    #[test]
    fn method_token_contract_drives_private_void_inference() {
        let manifest = standard_manifest();
        let script = [0x34, 0x06, 0x40, 0x21, 0x21, 0x21, 0x37, 0x00, 0x00, 0x40];
        let token = MethodToken {
            hash: [0; 20],
            method: "notify".to_string(),
            parameters_count: 0,
            has_return_value: false,
            call_flags: 0,
        };

        let contracts = analyze_with_tokens(&script, Some(&manifest), vec![token]);

        assert_eq!(
            contracts.get(6).map(|contract| contract.return_behavior),
            Some(ReturnBehavior::Void)
        );
    }

    #[test]
    fn manifest_declaration_overrides_private_return_inference_and_arity() {
        let manifest = manifest(
            r#"{
                "name": "DeclaredHelper",
                "abi": { "methods": [
                    { "name": "main", "parameters": [], "returntype": "Integer", "offset": 0 },
                    {
                        "name": "helper",
                        "parameters": [{ "name": "value", "type": "Integer" }],
                        "returntype": "Integer",
                        "offset": 4
                    }
                ] }
            }"#,
        );
        let script = [0x34, 0x04, 0x40, 0x21, 0x40];

        let contracts = analyze(&script, Some(&manifest));
        let helper = contracts.get(4).expect("declared helper contract");

        assert_eq!(helper.argument_count, 1);
        assert_eq!(helper.return_behavior, ReturnBehavior::Value);
    }

    #[test]
    fn manifest_void_declaration_overrides_value_left_on_stack() {
        let manifest = manifest(
            r#"{
                "name": "DeclaredVoidHelper",
                "abi": { "methods": [
                    { "name": "main", "parameters": [], "returntype": "Void", "offset": 0 },
                    {
                        "name": "helper",
                        "parameters": [],
                        "returntype": "Void",
                        "offset": 4
                    }
                ] }
            }"#,
        );
        let script = [0x34, 0x04, 0x40, 0x21, 0x11, 0x40];

        let contracts = analyze(&script, Some(&manifest));

        assert_eq!(
            contracts.get(4).map(|contract| contract.return_behavior),
            Some(ReturnBehavior::Void)
        );
    }

    #[test]
    fn offsetless_manifest_entry_uses_declared_contract() {
        let manifest = manifest(
            r#"{
                "name": "OffsetlessEntry",
                "abi": { "methods": [{
                    "name": "main",
                    "parameters": [
                        { "name": "left", "type": "Integer" },
                        { "name": "right", "type": "Integer" }
                    ],
                    "returntype": "Integer"
                }] }
            }"#,
        );

        let contracts = analyze(&[0x40], Some(&manifest));
        let entry = contracts.get(0).expect("entry contract");

        assert_eq!(entry.method.name, "main");
        assert_eq!(entry.argument_count, 2);
        assert_eq!(entry.return_behavior, ReturnBehavior::Value);
    }

    #[test]
    fn sorts_and_deduplicates_call_graph_methods_by_offset() {
        let manifest = standard_manifest();
        let instructions = Disassembler::new()
            .disassemble(PRIVATE_VOID_LEAF)
            .expect("script disassembles");
        let nef = NefFile {
            header: NefHeader {
                magic: *b"NEF3",
                compiler: "test".to_string(),
                source: String::new(),
            },
            method_tokens: Vec::new(),
            script: PRIVATE_VOID_LEAF.to_vec(),
            checksum: 0,
        };
        let mut call_graph = build_call_graph(&nef, &instructions, Some(&manifest));
        let duplicate = call_graph.methods[1].clone();
        call_graph.methods.reverse();
        call_graph.methods.push(duplicate);

        let contracts = infer_method_contracts(&instructions, Some(&manifest), &call_graph);
        let offsets: Vec<_> = contracts
            .methods
            .iter()
            .map(|contract| contract.method.offset)
            .collect();

        assert_eq!(offsets, vec![0, 7]);
    }

    #[test]
    fn serializes_return_behaviors_as_lowercase_strings() {
        let contracts = MethodContracts {
            methods: vec![
                contract(0, ReturnBehavior::Value),
                contract(1, ReturnBehavior::Void),
                contract(2, ReturnBehavior::Unknown),
            ],
        };

        let value = serde_json::to_value(contracts).expect("contracts serialize");
        let behaviors: Vec<_> = value["methods"]
            .as_array()
            .expect("methods array")
            .iter()
            .map(|method| method["return_behavior"].as_str().expect("behavior"))
            .collect();

        assert_eq!(behaviors, vec!["value", "void", "unknown"]);
    }

    #[test]
    fn get_returns_contract_at_requested_offset() {
        let contracts = MethodContracts {
            methods: vec![contract(2, ReturnBehavior::Unknown)],
        };

        assert_eq!(contracts.get(2), contracts.methods.first());
        assert_eq!(contracts.get(3), None);
    }

    #[test]
    fn map_projections_include_all_contracts_and_treat_unknown_as_value() {
        let contracts = MethodContracts {
            methods: vec![
                MethodContract {
                    argument_count: 3,
                    ..contract(0, ReturnBehavior::Value)
                },
                MethodContract {
                    argument_count: 2,
                    ..contract(1, ReturnBehavior::Void)
                },
                MethodContract {
                    argument_count: 1,
                    ..contract(2, ReturnBehavior::Unknown)
                },
            ],
        };

        assert_eq!(
            contracts.argument_counts_by_offset(),
            BTreeMap::from([(0, 3), (1, 2), (2, 1)])
        );
        assert_eq!(
            contracts.returns_value_by_offset(),
            BTreeMap::from([(0, true), (1, false), (2, true)])
        );
    }

    fn contract(offset: usize, return_behavior: ReturnBehavior) -> MethodContract {
        MethodContract {
            method: MethodRef {
                offset,
                name: format!("method_{offset}"),
            },
            argument_count: 0,
            return_behavior,
        }
    }
}
