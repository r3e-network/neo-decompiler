use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::analysis::call_graph::{build_call_graph, CallEdge, CallGraph, CallTarget};
use crate::decompiler::analysis::MethodRef;
use crate::disassembler::Disassembler;
use crate::manifest::ContractManifest;
use crate::nef::{MethodToken, NefFile, NefHeader};

use super::collection::{
    aggregate_private_argument_facts, intersect_static_writes, MethodCollectionAnalysis,
};
use super::{
    infer_method_contracts, CollectionArgumentEffect, CollectionShape, CollectionShapeFacts,
    MethodContract, MethodContracts, ReturnBehavior, SsaCollectionAnalysis, StaticCollectionWrite,
};

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
fn infers_fixed_struct_shape_from_all_reachable_returns() {
    let manifest = manifest(
        r#"{
                "name": "StructReturn",
                "abi": { "methods": [
                    {"name":"main","parameters":[],"returntype":"Array","offset":0},
                    {"name":"pair","parameters":[],"returntype":"Array","offset":4}
                ] }
            }"#,
    );
    let script = [0x34, 0x04, 0x40, 0x21, 0x11, 0x12, 0x12, 0xBF, 0x40];

    let contracts = analyze(&script, Some(&manifest));

    assert_eq!(
        contracts.get(4).expect("pair contract").return_shape,
        Some(CollectionShape::Struct(2))
    );
}

#[test]
fn infers_nested_private_entry_facts_through_static_constructor_chain() {
    let manifest = manifest(
        r#"{
                "name": "NestedStatic",
                "abi": { "methods": [
                    {"name":"main","parameters":[],"returntype":"Void","offset":0}
                ] }
            }"#,
    );
    let script = [
        0x0B, 0x0B, 0x12, 0xC0, 0x4A, 0x34, 0x07, 0x60, 0x58, 0x34, 0x0E, 0x40, 0x57, 0x00, 0x01,
        0x78, 0x10, 0x11, 0x11, 0x12, 0xC0, 0xD0, 0x40, 0x57, 0x00, 0x01, 0x78, 0x10, 0xCE, 0xC1,
        0x45, 0x45, 0x45, 0x40,
    ];

    let contracts = analyze(&script, Some(&manifest));

    assert_eq!(
        contracts.static_collection_facts.get(&0),
        Some(&CollectionShapeFacts {
            shape: Some(CollectionShape::Array(2)),
            indexed: BTreeMap::from([(0, CollectionShape::Array(2))]),
        })
    );
    let constructor = contracts.get(12).expect("constructor contract");
    assert_eq!(
        constructor.argument_field_writes,
        vec![BTreeMap::from([(0, CollectionShape::Array(2))])]
    );
    let consumer = contracts.get(23).expect("consumer contract");
    assert_eq!(
        consumer.argument_effects,
        vec![CollectionArgumentEffect::ReadOnly]
    );
    assert_eq!(
        consumer.argument_collection_facts,
        vec![CollectionShapeFacts {
            shape: Some(CollectionShape::Array(2)),
            indexed: BTreeMap::from([(0, CollectionShape::Array(2))]),
        }]
    );
}

#[test]
fn distinguishes_shape_preserving_and_resizing_argument_effects() {
    let manifest = standard_manifest();
    let shape_preserving = [
        0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x03, 0x79, 0x78, 0x10, 0x51, 0xD0, 0x7A, 0x78, 0x11,
        0x51, 0xD0, 0x40,
    ];
    let resizing = [
        0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x01, 0x78, 0x11, 0xCF, 0x40,
    ];

    let preserving_contracts = analyze(&shape_preserving, Some(&manifest));
    let resizing_contracts = analyze(&resizing, Some(&manifest));

    assert_eq!(
        preserving_contracts
            .get(4)
            .expect("SETITEM helper contract")
            .argument_effects,
        vec![
            CollectionArgumentEffect::PreservesShape,
            CollectionArgumentEffect::Unknown,
            CollectionArgumentEffect::Unknown,
        ]
    );
    assert_eq!(
        resizing_contracts
            .get(4)
            .expect("APPEND helper contract")
            .argument_effects,
        vec![CollectionArgumentEffect::Unknown]
    );
}

#[test]
fn returned_argument_alias_does_not_preserve_collection_shape() {
    let manifest = standard_manifest();
    let identity = [0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x01, 0x78, 0x40];

    let contracts = analyze(&identity, Some(&manifest));

    assert_eq!(
        contracts
            .get(4)
            .expect("identity helper contract")
            .argument_effects,
        vec![CollectionArgumentEffect::Unknown]
    );
}

#[test]
fn known_zero_argument_syscall_does_not_hide_shape_preserving_receiver() {
    let manifest = standard_manifest();
    let script = [
        0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x01, 0x41, 0x9B, 0xF6, 0x67, 0xCE, 0x45, 0x78, 0x10,
        0x11, 0x11, 0x12, 0xC0, 0xD0, 0x40,
    ];

    let contracts = analyze(&script, Some(&manifest));

    assert_eq!(
        contracts
            .get(4)
            .expect("syscall constructor contract")
            .argument_effects,
        vec![CollectionArgumentEffect::PreservesShape]
    );
}

#[test]
fn static_and_nested_argument_aliases_remain_unknown() {
    let manifest = standard_manifest();
    let static_escape = [0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x01, 0x78, 0x60, 0x40];
    let nested_alias = [
        0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x01, 0x78, 0x11, 0xC0, 0x45, 0x40,
    ];

    for script in [&static_escape[..], &nested_alias[..]] {
        assert_eq!(
            analyze(script, Some(&manifest))
                .get(4)
                .expect("escaping helper contract")
                .argument_effects,
            vec![CollectionArgumentEffect::Unknown]
        );
    }
}

#[test]
fn converted_argument_aliases_escape_shape_preservation() {
    let manifest = standard_manifest();
    // Each helper first mutates arg0 without resizing it, then aliases the
    // converted value through a return, a static, or SETITEM respectively.
    let returned = [
        0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x01, 0x78, 0xD1, 0x78, 0xDB, 0x40, 0x40,
    ];
    let stored_static = [
        0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x01, 0x78, 0xD1, 0x78, 0xDB, 0x40, 0x60, 0x40,
    ];
    let stored_nested = [
        0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x01, 0x78, 0xD1, 0x78, 0xDB, 0x40, 0xC2, 0x10, 0x51,
        0xD0, 0x40,
    ];

    for script in [&returned[..], &stored_static[..], &stored_nested[..]] {
        assert_eq!(
            analyze(script, Some(&manifest))
                .get(4)
                .expect("converted alias helper contract")
                .argument_effects,
            vec![CollectionArgumentEffect::Unknown]
        );
    }
}

#[test]
fn converted_argument_alias_passed_to_method_token_is_unknown() {
    let manifest = standard_manifest();
    let script = [
        0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x01, 0x78, 0xD1, 0x78, 0xDB, 0x40, 0x37, 0x00, 0x00,
        0x40,
    ];
    let token = MethodToken {
        hash: [0; 20],
        method: "consume".to_string(),
        parameters_count: 1,
        has_return_value: false,
        call_flags: 0,
    };

    let contracts = analyze_with_tokens(&script, Some(&manifest), vec![token]);

    assert_eq!(
        contracts
            .get(4)
            .expect("method-token alias helper contract")
            .argument_effects,
        vec![CollectionArgumentEffect::Unknown]
    );
}

#[test]
fn static_fact_intersection_rejects_unknown_and_conflicting_writes() {
    let array_two = CollectionShapeFacts {
        shape: Some(CollectionShape::Array(2)),
        indexed: BTreeMap::new(),
    };
    let known = StaticCollectionWrite {
        index: 0,
        facts: Some(array_two.clone()),
        is_null: false,
        provisional: false,
    };
    let null = StaticCollectionWrite {
        index: 0,
        facts: None,
        is_null: true,
        provisional: false,
    };
    assert_eq!(
        intersect_static_writes(&[null, known.clone()]),
        Some(array_two)
    );
    assert_eq!(
        intersect_static_writes(&[
            known.clone(),
            StaticCollectionWrite {
                index: 0,
                facts: Some(CollectionShapeFacts {
                    shape: Some(CollectionShape::Array(3)),
                    indexed: BTreeMap::new(),
                }),
                is_null: false,
                provisional: false,
            },
        ]),
        None
    );
    assert_eq!(
        intersect_static_writes(&[
            known,
            StaticCollectionWrite {
                index: 0,
                facts: None,
                is_null: false,
                provisional: false,
            },
        ]),
        None
    );
}

#[test]
fn private_entry_facts_require_every_direct_incoming_call_and_exclude_public_entries() {
    let target = MethodRef {
        offset: 20,
        name: "target".to_string(),
    };
    let graph = CallGraph {
        methods: vec![target.clone()],
        edges: vec![
            CallEdge {
                caller: MethodRef {
                    offset: 0,
                    name: "left".to_string(),
                },
                call_offset: 5,
                opcode: "CALL".to_string(),
                target: CallTarget::Internal {
                    method: target.clone(),
                },
            },
            CallEdge {
                caller: MethodRef {
                    offset: 10,
                    name: "right".to_string(),
                },
                call_offset: 15,
                opcode: "CALL_L".to_string(),
                target: CallTarget::Internal { method: target },
            },
        ],
    };
    let fact = CollectionShapeFacts {
        shape: Some(CollectionShape::Array(2)),
        indexed: BTreeMap::from([(0, CollectionShape::Struct(2))]),
    };
    let mut target_contract = contract(20, ReturnBehavior::Void);
    target_contract.argument_count = 1;
    target_contract.argument_collection_facts = vec![CollectionShapeFacts::default()];
    target_contract.argument_field_writes = vec![BTreeMap::new()];
    target_contract.argument_effects = vec![CollectionArgumentEffect::Unknown];
    let contracts = BTreeMap::from([(20, target_contract)]);
    let analysis = |call_offset| MethodCollectionAnalysis {
        trustworthy: true,
        analysis: SsaCollectionAnalysis {
            call_argument_facts: BTreeMap::from([(call_offset, vec![fact.clone()])]),
            ..SsaCollectionAnalysis::default()
        },
    };
    let mut analyses = BTreeMap::from([(0, analysis(5)), (10, analysis(15))]);

    assert_eq!(
        aggregate_private_argument_facts(
            &graph,
            &contracts,
            &analyses,
            &BTreeSet::new(),
            &BTreeSet::new(),
        )[&20],
        vec![fact.clone()]
    );

    analyses
        .get_mut(&10)
        .expect("right analysis")
        .analysis
        .call_argument_facts
        .insert(15, vec![CollectionShapeFacts::default()]);
    assert_eq!(
        aggregate_private_argument_facts(
            &graph,
            &contracts,
            &analyses,
            &BTreeSet::new(),
            &BTreeSet::new(),
        )[&20],
        vec![CollectionShapeFacts::default()]
    );

    let excluded = BTreeSet::from([20]);
    assert_eq!(
        aggregate_private_argument_facts(
            &graph,
            &contracts,
            &BTreeMap::from([(0, analysis(5)), (10, analysis(15))]),
            &excluded,
            &BTreeSet::new(),
        )[&20],
        vec![CollectionShapeFacts::default()]
    );
    assert_eq!(
        aggregate_private_argument_facts(
            &graph,
            &contracts,
            &BTreeMap::from([(0, analysis(5)), (10, analysis(15))]),
            &BTreeSet::new(),
            &excluded,
        )[&20],
        vec![CollectionShapeFacts::default()]
    );
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
        static_collection_facts: BTreeMap::new(),
    };

    let value = serde_json::to_value(contracts).expect("contracts serialize");
    let behaviors: Vec<_> = value["methods"]
        .as_array()
        .expect("methods array")
        .iter()
        .map(|method| method["return_behavior"].as_str().expect("behavior"))
        .collect();

    assert_eq!(behaviors, vec!["value", "void", "unknown"]);
    assert!(value["methods"]
        .as_array()
        .expect("methods array")
        .iter()
        .all(|method| method["may_return"] == true));
}

#[test]
fn infers_non_returning_effect_through_manifest_wrapper() {
    let manifest = manifest(
        r#"{
                "name": "AbortWrapper",
                "abi": { "methods": [
                    {"name":"main","parameters":[],"returntype":"Integer","offset":0},
                    {"name":"abortLeaf","parameters":[],"returntype":"Integer","offset":4}
                ] }
            }"#,
    );
    let contracts = analyze(&[0x34, 0x04, 0x40, 0x21, 0x38], Some(&manifest));

    let main = contracts.get(0).expect("main contract");
    let leaf = contracts.get(4).expect("leaf contract");
    assert_eq!(main.return_behavior, ReturnBehavior::Value);
    assert_eq!(leaf.return_behavior, ReturnBehavior::Value);
    assert!(!main.may_return);
    assert!(!leaf.may_return);
}

#[test]
fn keeps_may_return_when_any_reachable_path_returns() {
    let manifest = manifest(
        r#"{
                "name": "ConditionalAbort",
                "abi": { "methods": [
                    {"name":"main","parameters":[],"returntype":"Integer","offset":0},
                    {"name":"abortLeaf","parameters":[],"returntype":"Integer","offset":8}
                ] }
            }"#,
    );
    let script = [0x11, 0x24, 0x04, 0x11, 0x40, 0x34, 0x03, 0x40, 0x38];
    let contracts = analyze(&script, Some(&manifest));

    assert!(contracts.get(0).expect("main contract").may_return);
    assert!(!contracts.get(8).expect("leaf contract").may_return);
}

#[test]
fn get_returns_contract_at_requested_offset() {
    let contracts = MethodContracts {
        methods: vec![contract(2, ReturnBehavior::Unknown)],
        static_collection_facts: BTreeMap::new(),
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
        static_collection_facts: BTreeMap::new(),
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
        may_return: true,
        return_shape: None,
        argument_effects: vec![CollectionArgumentEffect::Unknown; 0],
        argument_collection_facts: Vec::new(),
        argument_field_writes: Vec::new(),
    }
}
