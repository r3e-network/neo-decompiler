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

#[test]
fn rewrite_indexing_syntax_rewrites_conditions_and_assignments() {
    let mut statements = vec![
        "if t0 get t1 {".to_string(),
        "}".to_string(),
        "else if t0 has_key t1 {".to_string(),
        "}".to_string(),
        "while t0 get t1 {".to_string(),
        "}".to_string(),
        "for (let i = 0; t0 has_key i; i = i + 1) {".to_string(),
        "}".to_string(),
        "let value = t0 get t1;".to_string(),
        "let nested = t0 get t1 get t2;".to_string(),
        "set_item(t0, t1, t2);".to_string(),
        "// if t0 get t1 {".to_string(),
    ];

    HighLevelEmitter::rewrite_indexing_syntax(&mut statements);

    assert_eq!(statements[0], "if t0[t1] {");
    assert_eq!(statements[2], "else if has_key(t0, t1) {");
    assert_eq!(statements[4], "while t0[t1] {");
    assert_eq!(
        statements[6],
        "for (let i = 0; has_key(t0, i); i = i + 1) {"
    );
    assert_eq!(statements[8], "let value = t0[t1];");
    assert_eq!(statements[9], "let nested = t0[t1[t2]];");
    assert_eq!(statements[10], "t0[t1] = t2;");
    assert_eq!(statements[11], "// if t0 get t1 {");
}

#[test]
fn rewrite_switch_statements_supports_temp_case_values() {
    let mut statements = vec![
        "let t1 = 0;".to_string(),
        "if loc0 == t1 {".to_string(),
        "    do0;".to_string(),
        "}".to_string(),
        "else if loc0 == 1 {".to_string(),
        "    do1;".to_string(),
        "}".to_string(),
        "else {".to_string(),
        "    do2;".to_string(),
        "}".to_string(),
    ];

    HighLevelEmitter::rewrite_switch_statements(&mut statements);

    assert_eq!(statements[0], "let t1 = 0;");
    assert_eq!(statements[1], "switch loc0 {");
    assert!(statements.iter().any(|line| line.trim() == "case 0 {"));
    assert!(statements.iter().any(|line| line.trim() == "case 1 {"));
    assert!(statements.iter().any(|line| line.trim() == "default {"));
    assert!(statements.iter().any(|line| line.trim() == "do0;"));
    assert!(statements.iter().any(|line| line.trim() == "do1;"));
    assert!(statements.iter().any(|line| line.trim() == "do2;"));
}

#[test]
fn rewrite_switch_statements_supports_string_literal_case_values() {
    let mut statements = vec![
        "let t0 = \"0\";".to_string(),
        "let t1 = loc0 == t0;".to_string(),
        "if t1 {".to_string(),
        "    do0;".to_string(),
        "}".to_string(),
        "let t2 = \"1\";".to_string(),
        "let t3 = loc0 == t2;".to_string(),
        "if t3 {".to_string(),
        "    do1;".to_string(),
        "}".to_string(),
        "let t4 = \"2\";".to_string(),
        "let t5 = loc0 == t4;".to_string(),
        "if t5 {".to_string(),
        "    do2;".to_string(),
        "}".to_string(),
    ];

    HighLevelEmitter::rewrite_switch_statements(&mut statements);

    assert!(statements.iter().any(|line| line.trim() == "switch loc0 {"));
    assert!(statements.iter().any(|line| line.trim() == "case \"0\" {"));
    assert!(statements.iter().any(|line| line.trim() == "case \"1\" {"));
    assert!(statements.iter().any(|line| line.trim() == "case \"2\" {"));
}

#[test]
fn rewrite_switch_statements_rewrites_long_guarded_goto_chains() {
    let mut statements = vec![
        "let loc0 = input;".to_string(),
        "let t0 = loc0 == 0;".to_string(),
        "if t0 { goto label_case_0; }".to_string(),
        "let t1 = loc0 == 1;".to_string(),
        "if t1 { goto label_case_1; }".to_string(),
        "let t2 = loc0 == 2;".to_string(),
        "if t2 { goto label_case_2; }".to_string(),
        "let t3 = loc0 == 3;".to_string(),
        "if t3 { goto label_case_3; }".to_string(),
        "let t4 = loc0 == 4;".to_string(),
        "if t4 { goto label_case_4; }".to_string(),
        "let t5 = loc0 == 5;".to_string(),
        "if t5 { goto label_case_5; }".to_string(),
        "let t6 = loc0 == 6;".to_string(),
        "if t6 { goto label_case_6; }".to_string(),
        "let t7 = loc0 == 7;".to_string(),
        "if !t7 {".to_string(),
        "    goto label_default;".to_string(),
        "    label_case_0:".to_string(),
        "    return 0;".to_string(),
        "    label_case_1:".to_string(),
        "    return 1;".to_string(),
        "    label_case_2:".to_string(),
        "    return 2;".to_string(),
        "    label_case_3:".to_string(),
        "    return 3;".to_string(),
        "    label_case_4:".to_string(),
        "    return 4;".to_string(),
        "    label_case_5:".to_string(),
        "    return 5;".to_string(),
        "    label_case_6:".to_string(),
        "    return 6;".to_string(),
        "}".to_string(),
        "return 7;".to_string(),
        "label_default:".to_string(),
        "return 99;".to_string(),
    ];

    HighLevelEmitter::rewrite_switch_statements(&mut statements);

    assert!(statements.iter().any(|line| line.trim() == "switch loc0 {"));
    assert!(statements.iter().any(|line| line.trim() == "case 0 {"));
    assert!(statements.iter().any(|line| line.trim() == "case 7 {"));
    assert!(statements.iter().any(|line| line.trim() == "default {"));
    assert!(!statements
        .iter()
        .any(|line| line.trim() == "if t0 { goto label_case_0; }"));
}

#[test]
fn rewrite_switch_statements_rewrites_guarded_chain_with_else_embedded_default_label() {
    let mut statements = vec![
        "let loc0 = input;".to_string(),
        "let t0 = loc0 == 0;".to_string(),
        "if t0 { goto label_case_0; }".to_string(),
        "let t1 = loc0 == 1;".to_string(),
        "if t1 { goto label_case_1; }".to_string(),
        "let t2 = loc0 == 2;".to_string(),
        "if t2 { goto label_case_2; }".to_string(),
        "let t3 = loc0 == 3;".to_string(),
        "if t3 { goto label_case_3; }".to_string(),
        "let t4 = loc0 == 4;".to_string(),
        "if t4 { goto label_case_4; }".to_string(),
        "let t5 = loc0 == 5;".to_string(),
        "if !t5 {".to_string(),
        "    goto label_default;".to_string(),
        "    label_case_0:".to_string(),
        "    do0;".to_string(),
        "    goto label_end;".to_string(),
        "    label_case_1:".to_string(),
        "    do1;".to_string(),
        "    goto label_end;".to_string(),
        "    label_case_2:".to_string(),
        "    do2;".to_string(),
        "    goto label_end;".to_string(),
        "    label_case_3:".to_string(),
        "    do3;".to_string(),
        "    goto label_end;".to_string(),
        "    label_case_4:".to_string(),
        "    do4;".to_string(),
        "    goto label_end;".to_string(),
        "}".to_string(),
        "else {".to_string(),
        "    do5;".to_string(),
        "    goto label_end;".to_string(),
        "    label_default:".to_string(),
        "    do_default;".to_string(),
        "}".to_string(),
        "label_end:".to_string(),
        "return;".to_string(),
    ];

    HighLevelEmitter::rewrite_switch_statements(&mut statements);

    assert!(statements.iter().any(|line| line.trim() == "switch loc0 {"));
    assert!(statements.iter().any(|line| line.trim() == "case 0 {"));
    assert!(statements.iter().any(|line| line.trim() == "case 5 {"));
    assert!(statements.iter().any(|line| line.trim() == "default {"));
    assert!(statements.iter().any(|line| line.trim() == "do5;"));
    assert!(statements.iter().any(|line| line.trim() == "do_default;"));
    assert!(
        !statements.iter().any(|line| line.trim() == "else {"),
        "switch rewrite should not leave dangling else wrappers in case bodies"
    );
}

#[test]
fn rewrite_switch_statements_rewrites_guarded_chain_with_else_case_and_external_default_label() {
    let mut statements = vec![
        "let loc0 = input;".to_string(),
        "let t0 = loc0 == 0;".to_string(),
        "if t0 { goto label_case_0; }".to_string(),
        "let t1 = loc0 == 1;".to_string(),
        "if t1 { goto label_case_1; }".to_string(),
        "let t2 = loc0 == 2;".to_string(),
        "if t2 { goto label_case_2; }".to_string(),
        "let t3 = loc0 == 3;".to_string(),
        "if t3 { goto label_case_3; }".to_string(),
        "let t4 = loc0 == 4;".to_string(),
        "if t4 { goto label_case_4; }".to_string(),
        "let t5 = loc0 == 5;".to_string(),
        "if !t5 {".to_string(),
        "    goto label_default;".to_string(),
        "    label_case_0:".to_string(),
        "    do0;".to_string(),
        "    goto label_end;".to_string(),
        "    label_case_1:".to_string(),
        "    do1;".to_string(),
        "    goto label_end;".to_string(),
        "    label_case_2:".to_string(),
        "    do2;".to_string(),
        "    goto label_end;".to_string(),
        "    label_case_3:".to_string(),
        "    do3;".to_string(),
        "    goto label_end;".to_string(),
        "    label_case_4:".to_string(),
        "    do4;".to_string(),
        "    goto label_end;".to_string(),
        "}".to_string(),
        "else {".to_string(),
        "    do5;".to_string(),
        "    goto label_end;".to_string(),
        "}".to_string(),
        "label_default:".to_string(),
        "do_default;".to_string(),
        "label_end:".to_string(),
        "return;".to_string(),
    ];

    HighLevelEmitter::rewrite_switch_statements(&mut statements);

    assert!(statements.iter().any(|line| line.trim() == "switch loc0 {"));
    assert!(statements.iter().any(|line| line.trim() == "case 0 {"));
    assert!(statements.iter().any(|line| line.trim() == "case 5 {"));
    assert!(statements.iter().any(|line| line.trim() == "default {"));
    assert!(statements.iter().any(|line| line.trim() == "do5;"));
    assert!(statements.iter().any(|line| line.trim() == "do_default;"));
    assert!(
        !statements.iter().any(|line| line.trim() == "else {"),
        "switch rewrite should not leave dangling else wrappers in case bodies"
    );
}

#[test]
fn rewrite_switch_statements_flattens_else_blocks_with_nested_chains() {
    let mut statements = vec![
        "if loc0 == 0 {".to_string(),
        "    do0;".to_string(),
        "}".to_string(),
        "else {".to_string(),
        "    if loc0 == 1 {".to_string(),
        "        do1;".to_string(),
        "    }".to_string(),
        "    else {".to_string(),
        "        do2;".to_string(),
        "    }".to_string(),
        "}".to_string(),
    ];

    HighLevelEmitter::rewrite_switch_statements(&mut statements);

    assert_eq!(statements[0], "switch loc0 {");
    assert!(statements.iter().any(|line| line.trim() == "case 0 {"));
    assert!(statements.iter().any(|line| line.trim() == "case 1 {"));
    assert!(statements.iter().any(|line| line.trim() == "default {"));
}

#[test]
fn rewrite_switch_statements_skips_duplicate_cases() {
    let mut statements = vec![
        "if loc0 == 0 {".to_string(),
        "    do0;".to_string(),
        "}".to_string(),
        "else if loc0 == 0 {".to_string(),
        "    do1;".to_string(),
        "}".to_string(),
        "else {".to_string(),
        "    do2;".to_string(),
        "}".to_string(),
    ];

    HighLevelEmitter::rewrite_switch_statements(&mut statements);

    assert_eq!(statements[0], "if loc0 == 0 {");
    assert!(!statements
        .iter()
        .any(|line| line.trim_start().starts_with("switch ")));
}

#[test]
fn rewrite_switch_statements_skips_non_literal_cases() {
    let mut statements = vec![
        "if loc0 == loc1 {".to_string(),
        "    do0;".to_string(),
        "}".to_string(),
        "else if loc0 == 1 {".to_string(),
        "    do1;".to_string(),
        "}".to_string(),
        "else {".to_string(),
        "    do2;".to_string(),
        "}".to_string(),
    ];

    HighLevelEmitter::rewrite_switch_statements(&mut statements);

    assert_eq!(statements[0], "if loc0 == loc1 {");
    assert!(!statements
        .iter()
        .any(|line| line.trim_start().starts_with("switch ")));
}

#[test]
fn rewrite_switch_statements_collapses_consecutive_standalone_ifs() {
    let mut statements = vec![
        "if loc0 == 0 {".to_string(),
        "    do0;".to_string(),
        "}".to_string(),
        "if loc0 == 1 {".to_string(),
        "    do1;".to_string(),
        "}".to_string(),
        "if loc0 == 2 {".to_string(),
        "    do2;".to_string(),
        "}".to_string(),
    ];

    HighLevelEmitter::rewrite_switch_statements(&mut statements);

    assert_eq!(statements[0], "switch loc0 {");
    assert!(statements.iter().any(|line| line.trim() == "case 0 {"));
    assert!(statements.iter().any(|line| line.trim() == "case 1 {"));
    assert!(statements.iter().any(|line| line.trim() == "case 2 {"));
    assert!(statements.iter().any(|line| line.trim() == "do0;"));
    assert!(statements.iter().any(|line| line.trim() == "do1;"));
    assert!(statements.iter().any(|line| line.trim() == "do2;"));
}

#[test]
fn rewrite_switch_statements_skips_two_consecutive_standalone_ifs() {
    let mut statements = vec![
        "if loc0 == 0 {".to_string(),
        "    do0;".to_string(),
        "}".to_string(),
        "if loc0 == 1 {".to_string(),
        "    do1;".to_string(),
        "}".to_string(),
    ];

    HighLevelEmitter::rewrite_switch_statements(&mut statements);

    // Only 2 cases — below the 3-case threshold for standalone ifs.
    assert_eq!(statements[0], "if loc0 == 0 {");
    assert!(!statements
        .iter()
        .any(|line| line.trim_start().starts_with("switch ")));
}

#[test]
fn rewrite_switch_statements_skips_consecutive_ifs_with_different_scrutinee() {
    let mut statements = vec![
        "if loc0 == 0 {".to_string(),
        "    do0;".to_string(),
        "}".to_string(),
        "if loc1 == 1 {".to_string(),
        "    do1;".to_string(),
        "}".to_string(),
        "if loc0 == 2 {".to_string(),
        "    do2;".to_string(),
        "}".to_string(),
    ];

    HighLevelEmitter::rewrite_switch_statements(&mut statements);

    assert_eq!(statements[0], "if loc0 == 0 {");
    assert!(!statements
        .iter()
        .any(|line| line.trim_start().starts_with("switch ")));
}
