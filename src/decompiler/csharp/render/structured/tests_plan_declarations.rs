use super::*;

#[test]
fn plans_declarations() {
    let body = Block::with_stmts(vec![
        Stmt::assign("t_0", Expr::int(1)),
        Stmt::expr(Expr::var("t_0")),
        Stmt::ControlFlow(Box::new(ControlFlow::if_else(
            Expr::var("cond"),
            Block::with_stmts(vec![Stmt::assign("loc0", Expr::int(1))]),
            Block::with_stmts(vec![Stmt::assign("loc0", Expr::int(2))]),
        ))),
        Stmt::expr(Expr::var("loc0")),
        Stmt::assign("loc1", Expr::int(0)),
        Stmt::ControlFlow(Box::new(ControlFlow::while_loop(
            Expr::binary(BinOp::Lt, Expr::var("loc1"), Expr::int(3)),
            Block::with_stmts(vec![Stmt::assign(
                "loc1",
                Expr::binary(BinOp::Add, Expr::var("loc1"), Expr::int(1)),
            )]),
        ))),
        Stmt::expr(Expr::var("loc1")),
        Stmt::assign("static1", Expr::var("loc0")),
        Stmt::expr(Expr::var("static1")),
        Stmt::expr(Expr::var("@class")),
        Stmt::expr(Expr::var("missing")),
    ]);
    let symbols = BTreeMap::from([
        (
            "cond".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Boolean,
            },
        ),
        (
            "@class".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(1),
                value_type: ValueType::Any,
            },
        ),
        (
            "loc0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(0),
                value_type: ValueType::Integer,
            },
        ),
        (
            "loc1".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(1),
                value_type: ValueType::Integer,
            },
        ),
        (
            "static1".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Static(1),
                value_type: ValueType::Integer,
            },
        ),
        (
            "t_0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
        (
            "missing".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
        (
            "static3".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Static(3),
                value_type: ValueType::Boolean,
            },
        ),
    ]);

    let plan = plan_declarations(&body, &symbols, true);
    let root_scope = plan.scopes.root();

    assert_eq!(plan.declarations["t_0"].scope, root_scope);
    assert_eq!(plan.declarations["t_0"].kind, DeclarationKind::Inline);
    assert_eq!(plan.declarations["loc0"].scope, root_scope);
    assert_eq!(
        plan.declarations["loc0"].kind,
        DeclarationKind::HoistedAssignment
    );
    assert_eq!(plan.declarations["loc1"].scope, root_scope);
    assert_eq!(
        plan.declarations["loc1"].kind,
        DeclarationKind::HoistedAssignment
    );
    assert!(!plan.declarations.contains_key("cond"));
    assert!(!plan.declarations.contains_key("@class"));
    assert!(!plan.declarations.contains_key("static1"));
    assert!(plan.issues.iter().any(|issue| {
        issue.kind == LoweringIssueKind::LostStackValue && issue.detail.contains("missing")
    }));

    let types = TypeInfo {
        statics: vec![ValueType::Unknown, ValueType::Integer],
        ..TypeInfo::default()
    };
    let contract = plan_contract_symbols(&types, &[&symbols], true, &BTreeSet::new());
    assert_eq!(contract.static_fields[1].name, "static1");
    assert_eq!(contract.static_fields[1].csharp_type, "BigInteger");
    let static3 = contract
        .static_fields
        .iter()
        .find(|field| field.name == "static3")
        .expect("referenced static beyond TypeInfo is planned");
    assert_eq!(static3.csharp_type, "bool");
}

#[test]
fn for_body_definition_used_by_update_is_hoisted_to_the_loop_scope() {
    let body = Block::with_stmts(vec![Stmt::ControlFlow(Box::new(ControlFlow::for_loop(
        None,
        Some(Expr::int(1)),
        Some(Expr::var("body_value")),
        Block::with_stmts(vec![Stmt::assign("body_value", Expr::int(1))]),
    )))]);
    let symbols = BTreeMap::from([(
        "body_value".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Integer,
        },
    )]);

    let plan = plan_declarations(&body, &symbols, true);
    let declaration = &plan.declarations["body_value"];

    assert_ne!(declaration.scope, plan.scopes.root());
    assert_eq!(declaration.kind, DeclarationKind::HoistedAssignment);
}

#[test]
fn missing_uninitialized_symbol_is_a_lost_stack_value() {
    let plan = plan_declarations(
        &Block::with_stmts(vec![Stmt::expr(Expr::var("orphan"))]),
        &BTreeMap::new(),
        true,
    );

    assert!(plan.issues.iter().any(|issue| {
        issue.kind == LoweringIssueKind::LostStackValue && issue.detail.contains("orphan")
    }));
    assert!(!plan
        .issues
        .iter()
        .any(|issue| issue.kind == LoweringIssueKind::MissingProvenance));
}

#[test]
fn stack_placeholder_is_a_lost_stack_value() {
    let plan = plan_declarations(
        &Block::with_stmts(vec![Stmt::expr(Expr::StackTemp(7))]),
        &BTreeMap::new(),
        true,
    );

    assert!(plan.issues.iter().any(|issue| {
        issue.kind == LoweringIssueKind::LostStackValue && issue.detail.contains('7')
    }));
    assert!(plan.declarations.is_empty());
}

#[test]
fn unused_local_copy_is_removed_without_dropping_its_source() {
    let body = Block::with_stmts(vec![
        Stmt::assign("source", Expr::int(1)),
        Stmt::assign("dead_copy", Expr::var("source")),
    ]);
    let symbols = BTreeMap::from([
        (
            "source".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
        (
            "dead_copy".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
    ]);

    let plan = plan_declarations(&body, &symbols, true);
    assert!(plan.unused_copy_symbols.contains("dead_copy"));
    assert!(plan.declarations.contains_key("dead_copy"));

    let rendered = render_block(&body, &plan, &symbols, ReturnBehavior::Void, false);
    assert!(rendered.contains("source = 1;"));
    assert!(!rendered.contains("dead_copy"));
}

#[test]
fn csharp_emits_static_referenced_beyond_type_info() {
    let instructions = vec![
        Instruction::new(0, OpCode::Ldsfld3, None),
        Instruction::new(1, OpCode::Ret, None),
    ];
    let nef = NefFile {
        header: NefHeader {
            magic: *b"NEF3",
            compiler: "test".to_string(),
            source: String::new(),
        },
        method_tokens: Vec::new(),
        script: vec![0x5B, 0x40],
        checksum: 0,
    };
    let rendered = super::super::super::render_csharp(
        &nef,
        &instructions,
        None,
        &CallGraph::default(),
        &MethodContracts::default(),
        &TypeInfo::default(),
        &RenderOptions {
            typed_declarations: true,
            ..RenderOptions::default()
        },
    )
    .source;

    assert!(
        rendered.contains("private static dynamic static3;"),
        "referenced static beyond TypeInfo must be emitted at class scope: {rendered}"
    );
}

#[test]
fn referenced_static_beyond_type_info_reserves_the_method_name() {
    let instructions = vec![
        Instruction::new(0, OpCode::Ldsfld3, None),
        Instruction::new(1, OpCode::Ret, None),
    ];
    let nef = NefFile {
        header: NefHeader {
            magic: *b"NEF3",
            compiler: "test".to_string(),
            source: String::new(),
        },
        method_tokens: Vec::new(),
        script: vec![0x5B, 0x40],
        checksum: 0,
    };
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "StaticCollision",
            "abi": { "methods": [{
                "name": "static3",
                "parameters": [],
                "returntype": "Any",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");
    let rendered = super::super::super::render_csharp(
        &nef,
        &instructions,
        Some(&manifest),
        &CallGraph::default(),
        &MethodContracts::default(),
        &TypeInfo::default(),
        &RenderOptions {
            typed_declarations: true,
            ..RenderOptions::default()
        },
    )
    .source;

    assert!(rendered.contains("private static dynamic static3;"));
    assert!(
        rendered.contains("public static object static3_1()"),
        "referenced static names must be reserved before methods are named: {rendered}"
    );
}

#[test]
fn static_fields_reserve_contract_member_names() {
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "StaticCollision",
            "abi": { "methods": [{
                "name": "static0",
                "parameters": [],
                "returntype": "Void",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");
    let types = TypeInfo {
        statics: vec![ValueType::Integer],
        ..TypeInfo::default()
    };

    let plans = build_csharp_method_plans(
        &[Instruction::new(0, OpCode::Ret, None)],
        Some(&manifest),
        &CallGraph::default(),
        &MethodContracts::default(),
        &types,
        &[0],
    );

    assert_eq!(plans[0].emitted_name, "static0_1");
}
