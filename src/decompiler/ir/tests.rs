//! Tests for the IR module.

use super::*;

#[test]
fn test_literal_rendering() {
    assert_eq!(render_expr(&Expr::int(42)), "42");
    assert_eq!(render_expr(&Expr::Literal(Literal::Bool(true))), "true");
    assert_eq!(
        render_expr(&Expr::Literal(Literal::String("hello".into()))),
        "\"hello\""
    );
    assert_eq!(render_expr(&Expr::Literal(Literal::Null)), "null");
    assert_eq!(
        render_expr(&Expr::Literal(Literal::Bytes(vec![0xDE, 0xAD]))),
        "0xdead"
    );
}

#[test]
fn test_variable_rendering() {
    assert_eq!(render_expr(&Expr::var("x")), "x");
    assert_eq!(render_expr(&Expr::var("local_0")), "local_0");
}

#[test]
fn test_binary_expression_rendering() {
    let expr = Expr::binary(BinOp::Add, Expr::var("x"), Expr::int(1));
    assert_eq!(render_expr(&expr), "(x + 1)");

    let nested = Expr::binary(
        BinOp::Mul,
        Expr::binary(BinOp::Add, Expr::var("a"), Expr::var("b")),
        Expr::var("c"),
    );
    assert_eq!(render_expr(&nested), "((a + b) * c)");
}

#[test]
fn test_unary_expression_rendering() {
    let neg = Expr::unary(UnaryOp::Neg, Expr::var("x"));
    assert_eq!(render_expr(&neg), "-x");

    let not = Expr::unary(UnaryOp::LogicalNot, Expr::var("flag"));
    assert_eq!(render_expr(&not), "!flag");

    let abs = Expr::unary(UnaryOp::Abs, Expr::var("n"));
    assert_eq!(render_expr(&abs), "abs(n)");
}

#[test]
fn test_call_expression_rendering() {
    let call = Expr::call("foo", vec![Expr::int(1), Expr::int(2)]);
    assert_eq!(render_expr(&call), "foo(1, 2)");

    let no_args = Expr::call("bar", vec![]);
    assert_eq!(render_expr(&no_args), "bar()");
}

#[test]
fn test_index_expression_rendering() {
    let idx = Expr::index(Expr::var("arr"), Expr::int(0));
    assert_eq!(render_expr(&idx), "arr[0]");
}

#[test]
fn test_array_rendering() {
    let arr = Expr::Array(vec![Expr::int(1), Expr::int(2), Expr::int(3)]);
    assert_eq!(render_expr(&arr), "[1, 2, 3]");
}

#[test]
fn test_assignment_statement_rendering() {
    let stmt = Stmt::assign("x", Expr::int(42));
    assert_eq!(render_stmt(&stmt, 0), "x = 42;");
    assert_eq!(render_stmt(&stmt, 1), "    x = 42;");
}

#[test]
fn test_return_statement_rendering() {
    let ret = Stmt::ret(Expr::var("result"));
    assert_eq!(render_stmt(&ret, 0), "return result;");

    let ret_void = Stmt::ret_void();
    assert_eq!(render_stmt(&ret_void, 0), "return;");
}

#[test]
fn test_comment_rendering() {
    let comment = Stmt::comment("this is a comment");
    assert_eq!(render_stmt(&comment, 0), "// this is a comment");
}

#[test]
fn test_unlifted_rendering() {
    let unlifted = Stmt::unlifted(0x0042, "PUSH1", "not yet translated");
    assert_eq!(
        render_stmt(&unlifted, 0),
        "// 0x0042: PUSH1 (not yet translated)"
    );
}

#[test]
fn test_block_rendering() {
    let block = Block::with_stmts(vec![
        Stmt::assign("x", Expr::int(1)),
        Stmt::assign("y", Expr::int(2)),
        Stmt::ret(Expr::binary(BinOp::Add, Expr::var("x"), Expr::var("y"))),
    ]);
    let rendered = render_block(&block, 0);
    assert!(rendered.contains("x = 1;"));
    assert!(rendered.contains("y = 2;"));
    assert!(rendered.contains("return (x + y);"));
}

#[test]
fn test_if_statement_rendering() {
    let if_stmt = ControlFlow::if_then(
        Expr::binary(BinOp::Gt, Expr::var("x"), Expr::int(0)),
        Block::with_stmts(vec![Stmt::ret(Expr::var("x"))]),
    );
    let stmt = Stmt::ControlFlow(Box::new(if_stmt));
    let rendered = render_stmt(&stmt, 0);
    assert!(rendered.contains("if ((x > 0))"));
    assert!(rendered.contains("return x;"));
}

#[test]
fn test_if_else_rendering() {
    let if_else = ControlFlow::if_else(
        Expr::var("flag"),
        Block::with_stmts(vec![Stmt::ret(Expr::int(1))]),
        Block::with_stmts(vec![Stmt::ret(Expr::int(0))]),
    );
    let stmt = Stmt::ControlFlow(Box::new(if_else));
    let rendered = render_stmt(&stmt, 0);
    assert!(rendered.contains("if (flag)"));
    assert!(rendered.contains("} else {"));
    assert!(rendered.contains("return 1;"));
    assert!(rendered.contains("return 0;"));
}

#[test]
fn test_while_loop_rendering() {
    let while_loop = ControlFlow::while_loop(
        Expr::binary(BinOp::Lt, Expr::var("i"), Expr::int(10)),
        Block::with_stmts(vec![Stmt::expr(Expr::unary(UnaryOp::Inc, Expr::var("i")))]),
    );
    let stmt = Stmt::ControlFlow(Box::new(while_loop));
    let rendered = render_stmt(&stmt, 0);
    assert!(rendered.contains("while ((i < 10))"));
}

#[test]
fn test_try_catch_rendering() {
    let try_catch = ControlFlow::try_catch(
        Block::with_stmts(vec![Stmt::expr(Expr::call("risky", vec![]))]),
        Some("e".into()),
        Some(Block::with_stmts(vec![Stmt::comment("handle error")])),
        Some(Block::with_stmts(vec![Stmt::comment("cleanup")])),
    );
    let stmt = Stmt::ControlFlow(Box::new(try_catch));
    let rendered = render_stmt(&stmt, 0);
    assert!(rendered.contains("try {"));
    assert!(rendered.contains("catch(e) {"));
    assert!(rendered.contains("finally {"));
}

#[test]
fn test_block_empty() {
    let block = Block::new();
    assert!(block.is_empty());
    assert_eq!(block.len(), 0);
}

#[test]
fn test_block_push() {
    let mut block = Block::new();
    block.push(Stmt::ret_void());
    assert!(!block.is_empty());
    assert_eq!(block.len(), 1);
}
