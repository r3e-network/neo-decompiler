use super::super::*;

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
