use super::super::super::HighLevelEmitter;
use super::super::util::extract_if_condition;

#[test]
fn collapses_else_if_chain() {
    let mut statements = vec![
        "if x > 0 {".to_string(),
        "foo()".to_string(),
        "}".to_string(),
        "else {".to_string(),
        "if x < 0 {".to_string(),
        "bar()".to_string(),
        "}".to_string(),
        "}".to_string(),
    ];
    HighLevelEmitter::rewrite_else_if_chains(&mut statements);
    assert!(statements.iter().any(|s| s.contains("else if x < 0")));
}

#[test]
fn preserves_simple_else() {
    let mut statements = vec![
        "if x > 0 {".to_string(),
        "foo()".to_string(),
        "}".to_string(),
        "else {".to_string(),
        "bar()".to_string(),
        "}".to_string(),
    ];
    let original = statements.clone();
    HighLevelEmitter::rewrite_else_if_chains(&mut statements);
    // Should not modify - else block doesn't start with if
    assert_eq!(statements.len(), original.len());
}

#[test]
fn extracts_if_condition() {
    assert_eq!(extract_if_condition("if x > 0 {"), Some("x > 0"));
    assert_eq!(extract_if_condition("if foo && bar {"), Some("foo && bar"));
    assert_eq!(extract_if_condition("while x {"), None);
}
