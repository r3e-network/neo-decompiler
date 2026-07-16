use super::super::*;

#[test]
fn collection_intrinsics_use_the_receiver_container_type() {
    let context = expr_context_with_types(&[
        ("items", ValueType::Array),
        ("map", ValueType::Map),
        ("bytes", ValueType::Buffer),
        ("text", ValueType::ByteString),
        ("index", ValueType::Integer),
        ("value", ValueType::Integer),
    ]);
    let intrinsic = |opcode, args| {
        Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        )
    };

    let cases = [
        (
            intrinsic(OpCode::Size, vec![Expr::var("items")]),
            "items.Length",
        ),
        (intrinsic(OpCode::Size, vec![Expr::var("map")]), "map.Count"),
        (
            intrinsic(OpCode::Size, vec![Expr::var("bytes")]),
            "bytes.Length",
        ),
        (
            intrinsic(OpCode::Append, vec![Expr::var("items"), Expr::var("value")]),
            "((Neo.SmartContract.Framework.List<object>)items).Add(value)",
        ),
        (
            intrinsic(OpCode::Remove, vec![Expr::var("items"), Expr::var("index")]),
            "((Neo.SmartContract.Framework.List<object>)items).RemoveAt((int)(index))",
        ),
        (
            intrinsic(OpCode::Remove, vec![Expr::var("map"), Expr::var("key")]),
            "map.Remove(key)",
        ),
        (
            intrinsic(OpCode::Clearitems, vec![Expr::var("items")]),
            "((Neo.SmartContract.Framework.List<object>)items).Clear()",
        ),
        (
            intrinsic(OpCode::Clearitems, vec![Expr::var("map")]),
            "map.Clear()",
        ),
        (
            intrinsic(
                OpCode::Pickitem,
                vec![Expr::var("items"), Expr::var("index")],
            ),
            "items[(int)(index)]",
        ),
        (
            intrinsic(OpCode::Pickitem, vec![Expr::var("map"), Expr::var("key")]),
            "map[key]",
        ),
        (
            intrinsic(
                OpCode::Setitem,
                vec![Expr::var("bytes"), Expr::var("index"), Expr::var("value")],
            ),
            "bytes[(int)(index)] = (byte)(dynamic)(value)",
        ),
        (
            intrinsic(
                OpCode::Setitem,
                vec![Expr::var("items"), Expr::var("index"), Expr::var("value")],
            ),
            "items[(int)(index)] = value",
        ),
        (
            intrinsic(
                OpCode::Setitem,
                vec![Expr::var("text"), Expr::var("index"), Expr::var("value")],
            ),
            "((byte[])(text))[(int)(index)] = (byte)(dynamic)(value)",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }

    let exact_context = ExprContext::default().with_concrete_types(&BTreeMap::from([
        ("text".to_string(), "ByteString".to_string()),
        ("buffer".to_string(), "byte[]".to_string()),
        ("number".to_string(), "BigInteger".to_string()),
    ]));
    assert_eq!(
        render_expr(
            &intrinsic(OpCode::Cat, vec![Expr::var("text"), Expr::var("buffer")]),
            &exact_context,
        ),
        "Helper.Concat(text, (ByteString)(buffer))"
    );
    assert_eq!(
        render_expr(
            &intrinsic(OpCode::Nz, vec![Expr::var("number")]),
            &exact_context,
        ),
        "number != 0"
    );
}

#[test]
fn ambiguous_collection_intrinsics_use_low_level_wrappers() {
    let context = ExprContext::default();
    let intrinsic = |opcode, args| {
        Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        )
    };
    let cases = [
        (
            intrinsic(OpCode::Size, vec![Expr::var("container")]),
            "(BigInteger)Runtime.LoadScript((ByteString)new byte[] { 0xCA }, CallFlags.All, new object[] { container })",
        ),
        (
            intrinsic(
                OpCode::Remove,
                vec![Expr::var("container"), Expr::var("key")],
            ),
            "Runtime.LoadScript((ByteString)new byte[] { 0xD2 }, CallFlags.All, new object[] { container, key })",
        ),
        (
            intrinsic(OpCode::Clearitems, vec![Expr::var("container")]),
            "Runtime.LoadScript((ByteString)new byte[] { 0xD3 }, CallFlags.All, new object[] { container })",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn indexing_intrinsics_guard_unsupported_receivers() {
    let context =
        expr_context_with_types(&[("flag", ValueType::Boolean), ("items", ValueType::Any)]);
    let intrinsic = |opcode, args| {
        Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        )
    };
    let cases = [
        (
            intrinsic(OpCode::Pickitem, vec![Expr::var("flag"), Expr::var("key")]),
            "((dynamic)(flag))[key]",
        ),
        (
            intrinsic(OpCode::Pickitem, vec![Expr::var("items"), Expr::var("key")]),
            "((dynamic)(items))[key]",
        ),
        (
            intrinsic(OpCode::Pickitem, vec![Expr::Unknown, Expr::var("key")]),
            "((dynamic)((dynamic)null))[key]",
        ),
        (
            intrinsic(
                OpCode::Setitem,
                vec![Expr::var("flag"), Expr::var("key"), Expr::var("value")],
            ),
            "((dynamic)(flag))[key] = value",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn byte_intrinsics_use_framework_compatible_conversions() {
    let context = expr_context_with_types(&[
        ("text", ValueType::ByteString),
        ("buffer", ValueType::Buffer),
        ("destination", ValueType::Buffer),
        ("integer", ValueType::Integer),
        ("items", ValueType::Array),
    ]);
    let intrinsic = |opcode, args| {
        Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        )
    };
    let cases = [
        (
            intrinsic(
                OpCode::Cat,
                vec![Expr::var("text"), Expr::var("buffer")],
            ),
            "Helper.Concat((ByteString)(text), (ByteString)(buffer))",
        ),
        (
            intrinsic(
                OpCode::Cat,
                vec![Expr::var("buffer"), Expr::var("text")],
            ),
            "Helper.Concat((byte[])(buffer), (ByteString)(text))",
        ),
        (
            intrinsic(
                OpCode::Cat,
                vec![Expr::var("integer"), Expr::var("items")],
            ),
            "Helper.Concat((ByteString)(dynamic)(integer), (ByteString)(dynamic)(items))",
        ),
        (
            intrinsic(
                OpCode::Substr,
                vec![Expr::var("text"), Expr::var("start"), Expr::var("length")],
            ),
            "(ByteString)(Helper.Range((byte[])(ByteString)(text), (int)(start), (int)(length)))",
        ),
        (
            intrinsic(
                OpCode::Left,
                vec![Expr::var("text"), Expr::var("count")],
            ),
            "(ByteString)(Helper.Take((byte[])(ByteString)(text), (int)(count)))",
        ),
        (
            intrinsic(
                OpCode::Right,
                vec![Expr::var("buffer"), Expr::var("count")],
            ),
            "Helper.Last((byte[])(buffer), (int)(count))",
        ),
        (
            intrinsic(
                OpCode::Memcpy,
                vec![
                    Expr::var("destination"),
                    Expr::var("destination_index"),
                    Expr::var("text"),
                    Expr::var("source_index"),
                    Expr::var("count"),
                ],
            ),
            "Array.Copy((byte[])(text), (int)(source_index), (byte[])(destination), (int)(destination_index), (int)(count))",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}
