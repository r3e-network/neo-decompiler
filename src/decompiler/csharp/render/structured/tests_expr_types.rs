use super::*;

#[test]
fn resolved_internal_call_return_types_drive_expression_typing() {
    let call = |offset| {
        Expr::call(
            SemanticCallTarget::Internal {
                offset,
                name: format!("helper_{offset}"),
            },
            Vec::new(),
        )
    };
    let context = ExprContext::default().with_internal_call_return_types(&BTreeMap::from([
        (1, "BigInteger".to_string()),
        (2, "bool".to_string()),
        (3, "ByteString".to_string()),
        (4, "byte[]".to_string()),
        (5, "Map<object, object>".to_string()),
        (6, "object[]".to_string()),
        (7, "dynamic".to_string()),
    ]));

    assert_eq!(context.value_type(&call(1)), ValueType::Integer);
    assert_eq!(context.value_type(&call(2)), ValueType::Boolean);
    assert_eq!(context.value_type(&call(3)), ValueType::ByteString);
    assert_eq!(context.value_type(&call(4)), ValueType::Buffer);
    assert_eq!(context.value_type(&call(5)), ValueType::Map);
    assert_eq!(context.value_type(&call(6)), ValueType::Array);
    assert_eq!(context.value_type(&call(7)), ValueType::Unknown);
    assert_eq!(context.value_type(&call(8)), ValueType::Unknown);

    let unresolved = Expr::unresolved_call("helper", Vec::new());
    assert_eq!(context.value_type(&unresolved), ValueType::Unknown);
}

#[test]
fn resolved_boolean_internal_call_avoids_dynamic_truthiness_cast() {
    let call = Expr::call(
        SemanticCallTarget::Internal {
            offset: 42,
            name: "predicate".to_string(),
        },
        Vec::new(),
    );
    let context = ExprContext::default()
        .with_internal_call_return_types(&BTreeMap::from([(42, "bool".to_string())]));

    assert_eq!(render_vm_condition(&call, &context), "predicate()");
    assert_eq!(
        render_expr(&Expr::unary(UnaryOp::LogicalNot, call), &context),
        "!predicate()"
    );
}

#[test]
fn proven_expression_shapes_keep_concrete_value_types() {
    let context = ExprContext::default();
    let typed_array = Expr::NewArray {
        length: Box::new(Expr::int(2)),
        element_type: Some(ValueType::Integer),
    };
    assert_eq!(
        context.value_type(&Expr::index(typed_array, Expr::int(0))),
        ValueType::Integer
    );
    assert_eq!(
        context.value_type(&Expr::Ternary {
            condition: Box::new(Expr::var("condition")),
            then_expr: Box::new(Expr::int(1)),
            else_expr: Box::new(Expr::int(2)),
        }),
        ValueType::Integer
    );
    assert_eq!(
        context.value_type(&Expr::Ternary {
            condition: Box::new(Expr::var("condition")),
            then_expr: Box::new(Expr::int(1)),
            else_expr: Box::new(Expr::Literal(Literal::Bool(false))),
        }),
        ValueType::Unknown
    );
    assert_eq!(
        context.value_type(&Expr::Cast {
            expr: Box::new(Expr::Unknown),
            target_type: "BigInteger".to_string(),
        }),
        ValueType::Integer
    );
    assert_eq!(
        context.value_type(&Expr::Cast {
            expr: Box::new(Expr::Unknown),
            target_type: "object[]".to_string(),
        }),
        ValueType::Array
    );
}
