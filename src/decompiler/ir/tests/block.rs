use super::super::*;

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
