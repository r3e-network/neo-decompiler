use super::*;

#[test]
fn reduce_double_parens_collapses_nested_pairs() {
    // The single-use-temp inliner unconditionally wraps multi-token
    // substitutions in parens. When the substitution lands inside a
    // call argument (or any already-parenthesised context), we end up
    // with `assert((x > 0))` and similar double parens. The pass
    // strips one redundant pair while preserving the operator's
    // precedence-safe outer parens.
    let mut statements = vec![
        "assert((x > 0));".to_string(),
        "let y = ((a + b));".to_string(),
        "if (((cond))) {".to_string(),
        "foo((x), (y));".to_string(), // these parens are NOT redundant
    ];

    HighLevelEmitter::reduce_double_parens(&mut statements);

    assert_eq!(statements[0], "assert(x > 0);");
    assert_eq!(statements[1], "let y = (a + b);");
    assert_eq!(statements[2], "if (cond) {");
    assert_eq!(
        statements[3], "foo((x), (y));",
        "function-call argument parens around different operands must not collapse"
    );
}

#[test]
fn eliminate_dead_temps_strips_unused_arithmetic_expression() {
    // After dispatch + inlining, an unused temp computed from a pure
    // expression should be removed. Previously only literal/identifier
    // RHS were considered "pure" and arithmetic temps survived as
    // `var tN = loc1 * 3;` lines that did nothing.
    let mut statements = vec!["let t1 = loc1 * 3;".to_string(), "return;".to_string()];

    HighLevelEmitter::eliminate_dead_temps(&mut statements);

    assert!(
        statements[0].is_empty(),
        "dead temp with arithmetic rhs should be cleared: {statements:?}"
    );
    assert_eq!(statements[1], "return;");
}

#[test]
fn eliminate_dead_temps_keeps_calls_for_their_side_effects() {
    // Function or syscall calls may have side effects (storage writes,
    // notifications, throws) so even an unused temp must stay.
    let mut statements = vec![
        "let t0 = syscall(\"System.Storage.Get\", t1);".to_string(),
        "return;".to_string(),
    ];

    HighLevelEmitter::eliminate_dead_temps(&mut statements);

    assert_eq!(
        statements[0], "let t0 = syscall(\"System.Storage.Get\", t1);",
        "temps holding call results must not be eliminated: {statements:?}"
    );
}

#[test]
fn eliminate_dead_temps_keeps_used_temps() {
    let mut statements = vec!["let t0 = loc1 + 1;".to_string(), "return t0;".to_string()];

    HighLevelEmitter::eliminate_dead_temps(&mut statements);

    assert_eq!(statements[0], "let t0 = loc1 + 1;");
    assert_eq!(statements[1], "return t0;");
}

#[test]
fn eliminate_dead_temps_keeps_potentially_throwing_division_or_indexing() {
    // DIV and MOD throw on zero; PICKITEM throws on out-of-bounds. An
    // unused temp built from these operators must not be silently dropped
    // because that would hide a runtime exception the bytecode would have
    // raised.
    let mut statements = vec![
        "let t0 = a / b;".to_string(),
        "let t1 = c % d;".to_string(),
        "let t2 = arr[i];".to_string(),
        "return;".to_string(),
    ];

    HighLevelEmitter::eliminate_dead_temps(&mut statements);

    assert_eq!(statements[0], "let t0 = a / b;");
    assert_eq!(statements[1], "let t1 = c % d;");
    assert_eq!(statements[2], "let t2 = arr[i];");
}

#[test]
fn eliminate_fallthrough_gotos_strips_goto_followed_by_label() {
    let mut statements = vec![
        "let x = 1;".to_string(),
        "goto label_0x0010;".to_string(),
        "label_0x0010:".to_string(),
        "return x;".to_string(),
    ];

    HighLevelEmitter::eliminate_fallthrough_gotos(&mut statements);

    assert!(
        statements[1].is_empty(),
        "fallthrough goto should be cleared: {statements:?}"
    );
    // The label remains; remove_orphaned_labels (a separate pass) is what
    // strips it once it has no remaining references.
    assert_eq!(statements[2], "label_0x0010:");
}

#[test]
fn eliminate_fallthrough_gotos_strips_leave_followed_by_label() {
    // `leave` is the high-level encoding of an ENDTRY transfer. When the
    // resume target sits immediately after the `leave`, the transfer is a
    // visual no-op the C# / Rust backends would emit identical control
    // flow for, so it should be stripped just like the `goto` form.
    let mut statements = vec![
        "try {".to_string(),
        "    leave label_0x0008;".to_string(),
        "    label_0x0008:".to_string(),
        "}".to_string(),
        "finally {".to_string(),
        "}".to_string(),
    ];

    HighLevelEmitter::eliminate_fallthrough_gotos(&mut statements);

    assert!(
        statements[1].is_empty(),
        "fallthrough leave should be cleared: {statements:?}"
    );
}

#[test]
fn eliminate_fallthrough_gotos_strips_leave_through_close_braces() {
    // The leave is the last statement of a catch body; the label sits
    // immediately after the `}`. Walking past the close-brace finds the
    // label, so the transfer is dead — control reaches the label by
    // structural fall-out anyway.
    let mut statements = vec![
        "try {".to_string(),
        "}".to_string(),
        "catch {".to_string(),
        "    leave label_0x0009;".to_string(),
        "}".to_string(),
        "label_0x0009:".to_string(),
        "return;".to_string(),
    ];

    HighLevelEmitter::eliminate_fallthrough_gotos(&mut statements);

    assert!(
        statements[3].is_empty(),
        "leave at end of catch body should be cleared when label sits past `}}`: {statements:?}"
    );
}

#[test]
fn eliminate_fallthrough_gotos_keeps_leave_when_intervening_code_present() {
    // Same shape but with executable code between the closing brace and
    // the label — eliminating the leave would now skip that code,
    // changing semantics. The transfer must stay.
    let mut statements = vec![
        "catch {".to_string(),
        "    leave label_0x0010;".to_string(),
        "}".to_string(),
        "let x = 1;".to_string(),
        "label_0x0010:".to_string(),
        "return;".to_string(),
    ];

    HighLevelEmitter::eliminate_fallthrough_gotos(&mut statements);

    assert_eq!(
        statements[1].trim(),
        "leave label_0x0010;",
        "leave with intervening code must be preserved: {statements:?}"
    );
}

#[test]
fn eliminate_fallthrough_gotos_keeps_leave_when_target_is_distant() {
    let mut statements = vec![
        "leave label_0x0010;".to_string(),
        "let unreachable = 1;".to_string(),
        "label_0x0010:".to_string(),
        "return;".to_string(),
    ];

    HighLevelEmitter::eliminate_fallthrough_gotos(&mut statements);

    assert_eq!(
        statements[0], "leave label_0x0010;",
        "non-fallthrough leave must be preserved: {statements:?}"
    );
}

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
