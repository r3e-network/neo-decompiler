use super::*;

#[test]
fn rewrite_for_loops_handles_temp_increment_chain() {
    let mut statements = vec![
        "let loc0 = 0;".to_string(),
        "while loc0 < 3 {".to_string(),
        "    // work".to_string(),
        "    temp1 = loc0 + 1;".to_string(),
        "    loc0 = temp1;".to_string(),
        "}".to_string(),
    ];

    HighLevelEmitter::rewrite_for_loops(&mut statements);

    assert_eq!(
        statements[1],
        "for (let loc0 = 0; loc0 < 3; loc0 = loc0 + 1) {"
    );
    assert!(statements[0].is_empty(), "initializer should be removed");
    assert!(statements[3].is_empty(), "temp increment should be removed");
    assert!(
        statements[4].is_empty(),
        "final increment should be removed"
    );
}

#[test]
fn rewrite_for_loops_handles_direct_increment() {
    let mut statements = vec![
        "let loc0 = t0;".to_string(),
        "while loc0 < limit {".to_string(),
        "    loc0 = loc0 + 1;".to_string(),
        "}".to_string(),
    ];

    HighLevelEmitter::rewrite_for_loops(&mut statements);

    assert_eq!(
        statements[1],
        "for (let loc0 = t0; loc0 < limit; loc0 = loc0 + 1) {"
    );
    assert!(statements[0].is_empty(), "initializer should be removed");
    assert!(statements[2].is_empty(), "increment should be removed");
}
