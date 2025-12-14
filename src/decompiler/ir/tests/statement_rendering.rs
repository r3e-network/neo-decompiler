use super::super::*;

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
