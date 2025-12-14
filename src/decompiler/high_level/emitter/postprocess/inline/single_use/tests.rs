use super::*;

#[test]
fn single_use_temp_is_inlined_into_first_use_site() {
    let mut statements = vec!["let t0 = x + 1;".to_string(), "let loc0 = t0;".to_string()];

    HighLevelEmitter::inline_single_use_temps(&mut statements);

    assert_eq!(statements[0], "");
    assert_eq!(statements[1], "let loc0 = (x + 1);");
}

#[test]
fn single_use_temp_is_not_inlined_into_control_flow_conditions() {
    let mut statements = vec!["let t0 = x == 0;".to_string(), "if t0 {".to_string()];

    HighLevelEmitter::inline_single_use_temps(&mut statements);

    assert_eq!(statements[0], "let t0 = x == 0;");
    assert_eq!(statements[1], "if t0 {");
}

#[test]
fn single_use_literal_temp_is_inlined_into_control_flow_conditions() {
    let mut statements = vec!["let t0 = 3;".to_string(), "if x < t0 {".to_string()];

    HighLevelEmitter::inline_single_use_temps(&mut statements);

    assert_eq!(statements[0], "");
    assert_eq!(statements[1], "if x < 3 {");
}

#[test]
fn non_temp_identifiers_are_not_inlined() {
    let mut statements = vec!["let loc0 = x + 1;".to_string(), "return loc0;".to_string()];

    HighLevelEmitter::inline_single_use_temps(&mut statements);

    assert_eq!(statements[0], "let loc0 = x + 1;");
    assert_eq!(statements[1], "return loc0;");
}

#[test]
fn temp_replacement_respects_identifier_boundaries() {
    let mut statements = vec!["let t1 = 1;".to_string(), "let x = t10 + t1;".to_string()];

    HighLevelEmitter::inline_single_use_temps(&mut statements);

    assert_eq!(statements[0], "");
    assert_eq!(statements[1], "let x = t10 + 1;");
}
