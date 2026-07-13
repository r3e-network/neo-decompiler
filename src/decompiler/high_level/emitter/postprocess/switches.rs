//! Rewrite `if` / `else if` equality chains into `switch` statements.
//!
//! This pass is intentionally conservative: it only rewrites chains that
//! compare the same scrutinee expression against literal case values.
//!
//! Two patterns are recognized:
//! - `if/else if` chains (minimum 2 cases)
//! - Consecutive standalone `if` blocks comparing the same variable (minimum 3 cases)

use super::super::HighLevelEmitter;
use super::util::{is_else_open, is_if_open};

mod chain;
mod guarded;

impl HighLevelEmitter {
    /// Rewrite eligible `if` / `else if` chains into `switch` blocks.
    pub(crate) fn rewrite_switch_statements(statements: &mut Vec<String>) {
        let mut index = 0usize;
        while index < statements.len() {
            if let Some((replacement, end)) =
                guarded::try_build_guarded_goto_switch(statements, index)
            {
                statements.splice(index..=end, replacement);
                index += 1;
                continue;
            }
            if let Some((replacement, end)) = chain::try_build_switch(statements, index) {
                statements.splice(index..=end, replacement);
                index += 1;
                continue;
            }
            index += 1;
        }
    }
}

const MIN_GUARDED_GOTO_CASES: usize = 2;

fn extract_block_body(statements: &[String], header_index: usize) -> Option<(Vec<String>, usize)> {
    let end = HighLevelEmitter::find_block_end(statements, header_index)?;
    let body = statements
        .get(header_index + 1..end)
        .unwrap_or_default()
        .to_vec();
    Some((body, end))
}

fn collect_trivia(statements: &[String], mut index: usize) -> (Vec<String>, usize) {
    let mut trivia = Vec::new();
    while index < statements.len() {
        let trimmed = statements[index].trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            trivia.push(statements[index].clone());
            index += 1;
            continue;
        }
        break;
    }
    (trivia, index)
}

fn resolve_condition_expression(
    statements: &[String],
    header_index: usize,
    condition: &str,
) -> Option<String> {
    if condition.contains("==") {
        return Some(condition.trim().to_string());
    }
    let condition = condition.trim();
    let condition = condition
        .strip_prefix('!')
        .map(str::trim)
        .unwrap_or(condition);
    let prev = HighLevelEmitter::previous_code_line(statements, header_index)?;
    let assign = HighLevelEmitter::parse_assignment(statements[prev].as_str())?;
    (assign.lhs == condition).then_some(assign.rhs)
}

fn parse_case_sides(condition: &str) -> Option<(String, &str)> {
    let (left, right) = split_equals(condition)?;
    let left = left.trim();
    let right = right.trim();

    if is_literal(left) && !is_literal(right) {
        return Some((right.to_string(), left));
    }
    if !is_literal(left) && is_literal(right) {
        return Some((left.to_string(), right));
    }

    // Common compiler shape: `loc0 == t1` where `t1` is a pushed literal.
    if is_temp(left) && !is_temp(right) {
        return Some((right.to_string(), left));
    }
    if is_temp(right) && !is_temp(left) {
        return Some((left.to_string(), right));
    }

    None
}

fn resolve_case_value(statements: &[String], header_index: usize, token: &str) -> Option<String> {
    if is_literal(token) {
        return Some(token.trim().to_string());
    }
    if !is_temp(token) {
        return None;
    }

    let mut cursor = header_index;
    while let Some(prev) = HighLevelEmitter::previous_code_line(statements, cursor) {
        cursor = prev;
        let Some(assign) = HighLevelEmitter::parse_assignment(statements[prev].as_str()) else {
            continue;
        };
        if assign.lhs != token {
            continue;
        }
        let rhs = assign.rhs.trim().to_string();
        return is_literal(rhs.as_str()).then_some(rhs);
    }
    None
}

fn split_equals(condition: &str) -> Option<(&str, &str)> {
    let pos = condition.find("==")?;
    let (left, rest) = condition.split_at(pos);
    let right = rest.strip_prefix("==")?;
    Some((left, right))
}

fn is_literal(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        return true;
    }
    if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 3 {
        return true;
    }
    if matches!(value, "true" | "false" | "null") {
        return true;
    }
    if value.starts_with("0x") && value.len() > 2 {
        return value[2..].chars().all(|ch| ch.is_ascii_hexdigit());
    }
    value.parse::<i64>().is_ok()
}

fn is_temp(value: &str) -> bool {
    // A temp identifier is `t` followed by at least one digit (`t0`, `t12`),
    // matching the JS port's `^t\d+$`. The non-empty digit requirement avoids
    // treating a bare `t` as a temp.
    let value = value.trim();
    value.len() > 1 && value.starts_with('t') && value[1..].bytes().all(|b| b.is_ascii_digit())
}

fn find_first_if_in_range(statements: &[String], start: usize, end: usize) -> Option<usize> {
    let mut index = start;
    while index < end {
        let trimmed = statements[index].trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            index += 1;
            continue;
        }
        if is_if_open(trimmed) {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn find_next_if_after_case_prelude(statements: &[String], start: usize) -> Option<usize> {
    let mut index = start;
    while index < statements.len() {
        let trimmed = statements[index].trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            index += 1;
            continue;
        }
        if is_if_open(trimmed) {
            return Some(index);
        }
        // Only skip a genuine case-value temp definition: a `tN = …` whose temp
        // feeds the upcoming case comparison (so it is referenced by the next
        // code statement). Anything else must block the fold so it is not
        // spliced away and silently dropped — a real local assignment
        // (`loc5 = effect();`) AND a temp that captures a side-effecting call
        // and discards it (`let t7 = Foo(arg);`, not consumed by the next case).
        if let Some(assign) = HighLevelEmitter::parse_assignment(statements[index].as_str()) {
            if is_temp(&assign.lhs) && temp_consumed_by_next_code(statements, index, &assign.lhs) {
                index += 1;
                continue;
            }
        }
        return None;
    }
    None
}

/// A case-value temp prelude (`tN = <value>;`) feeds the upcoming comparison, so
/// `tN` is referenced by the next code statement (e.g. `tN = loc0 == tM;` or
/// `if tN { … }`). A temp that captures a side-effecting call and is discarded
/// is not referenced; treat it as a real statement so the switch fold is blocked
/// and it is preserved.
fn temp_consumed_by_next_code(statements: &[String], index: usize, temp: &str) -> bool {
    statements
        .iter()
        .skip(index + 1)
        .find(|stmt| {
            let trimmed = stmt.trim();
            !trimmed.is_empty() && !trimmed.starts_with("//")
        })
        .is_some_and(|stmt| HighLevelEmitter::contains_identifier(stmt, temp))
}

fn find_next_guarded_header_after_case_prelude(
    statements: &[String],
    start: usize,
) -> Option<usize> {
    let mut index = start;
    while index < statements.len() {
        let trimmed = statements[index].trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            index += 1;
            continue;
        }
        if parse_inline_if_goto(trimmed).is_some() || is_if_open(trimmed) {
            return Some(index);
        }
        // See find_next_if_after_case_prelude: only skip a case-value temp that
        // feeds the upcoming comparison; a real inter-case assignment or a temp
        // capturing a discarded side-effecting call must block the fold so it is
        // not silently dropped.
        if let Some(assign) = HighLevelEmitter::parse_assignment(statements[index].as_str()) {
            if is_temp(&assign.lhs) && temp_consumed_by_next_code(statements, index, &assign.lhs) {
                index += 1;
                continue;
            }
        }
        return None;
    }
    None
}

/// A standalone-`if` case body is safe to fold into a `switch` only when it
/// cannot fall through into a later case's comparison: either it ends in a
/// terminator (so control never reaches the next `if`), or it never
/// reassigns the scrutinee (so a later `scrutinee == k` can't newly match).
fn case_body_is_switch_safe(body: &[String], scrutinee: &str) -> bool {
    if body_ends_with_terminator(body) {
        return true;
    }
    !body.iter().any(|line| statement_reassigns(line, scrutinee))
}

fn body_ends_with_terminator(body: &[String]) -> bool {
    for line in body.iter().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed == "{" || trimmed == "}" {
            continue;
        }
        return is_terminator_statement(trimmed);
    }
    false
}

fn is_terminator_statement(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "return;"
        || trimmed.starts_with("return ")
        || trimmed.starts_with("throw")
        || trimmed.starts_with("abort")
        || trimmed.starts_with("goto ")
        || trimmed == "break;"
        || trimmed == "continue;"
}

fn statement_reassigns(line: &str, scrutinee: &str) -> bool {
    HighLevelEmitter::parse_assignment(line).is_some_and(|assignment| assignment.lhs == scrutinee)
}

fn parse_inline_if_goto(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    let rest = line.strip_prefix("if ")?;
    let (condition, suffix) = rest.split_once(" { goto ")?;
    let label = suffix.strip_suffix("; }")?.trim();
    if label.is_empty() {
        return None;
    }
    Some((condition.trim().to_string(), label.to_string()))
}

fn parse_plain_goto_label(line: &str) -> Option<String> {
    let line = line.trim();
    let label = line.strip_prefix("goto ")?.strip_suffix(';')?.trim();
    if label.is_empty() {
        return None;
    }
    Some(label.to_string())
}

fn parse_label_line(line: &str) -> Option<String> {
    let line = line.trim();
    let label = line.strip_suffix(':')?.trim();
    if !label.starts_with("label_") {
        return None;
    }
    Some(label.to_string())
}

fn parse_guarded_switch_body_header(
    statements: &[String],
    start: usize,
    end: usize,
) -> Option<(String, usize)> {
    let (_, first_code) = collect_trivia(statements, start);
    if first_code >= end {
        return None;
    }
    let default_label = parse_plain_goto_label(statements[first_code].as_str())?;
    let (_, body_start) = collect_trivia(statements, first_code + 1);
    if body_start >= end {
        return None;
    }
    parse_label_line(statements[body_start].as_str())?;
    Some((default_label, body_start))
}

fn collect_label_bodies(
    statements: &[String],
    start: usize,
    end: usize,
) -> Option<std::collections::BTreeMap<String, Vec<String>>> {
    let mut bodies: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let mut current_label: Option<String> = None;
    let mut index = start;

    while index < end {
        let trimmed = statements[index].trim();
        if let Some(label) = parse_label_line(trimmed) {
            if bodies.contains_key(&label) {
                return None;
            }
            current_label = Some(label.clone());
            bodies.insert(label, Vec::new());
            index += 1;
            continue;
        }

        let Some(label) = current_label.as_ref() else {
            if trimmed.is_empty() || trimmed.starts_with("//") {
                index += 1;
                continue;
            }
            return None;
        };
        bodies
            .entry(label.clone())
            .or_default()
            .push(statements[index].clone());
        index += 1;
    }

    Some(bodies)
}

fn find_label_after(statements: &[String], start: usize, label: &str) -> Option<usize> {
    let needle = format!("{label}:");
    let mut index = start;
    while index < statements.len() {
        if statements[index].trim() == needle {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn find_label_in_range(
    statements: &[String],
    start: usize,
    end: usize,
    label: &str,
) -> Option<usize> {
    let needle = format!("{label}:");
    let mut index = start;
    while index < end {
        if statements[index].trim() == needle {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn find_else_block_after(statements: &[String], start: usize) -> Option<(usize, usize)> {
    let (_, header) = collect_trivia(statements, start);
    if header >= statements.len() || !is_else_open(statements[header].trim()) {
        return None;
    }
    let end = HighLevelEmitter::find_block_end(statements, header)?;
    Some((header, end))
}

fn find_label_body_end(statements: &[String], start: usize) -> usize {
    let mut index = start;
    while index < statements.len() {
        if index > start && parse_label_line(statements[index].as_str()).is_some() {
            break;
        }
        index += 1;
    }
    index.saturating_sub(1)
}

fn end_of_if_chain(statements: &[String], start: usize) -> Option<usize> {
    let if_end = HighLevelEmitter::find_block_end(statements, start)?;
    let (_, next) = collect_trivia(statements, if_end + 1);
    if next < statements.len() && is_else_open(statements[next].trim()) {
        return HighLevelEmitter::find_block_end(statements, next);
    }
    Some(if_end)
}

#[cfg(test)]
mod tests {
    use super::super::super::HighLevelEmitter;

    #[test]
    fn switch_fold_preserves_non_temp_inter_case_statement() {
        // Regression: a real (non-temp) assignment between consecutive
        // standalone-if cases must block the switch fold so it is not silently
        // spliced away and dropped from the output.
        let mut statements = vec![
            "if loc0 == 0 {".to_string(),
            "    do0();".to_string(),
            "}".to_string(),
            "loc5 = side_effect();".to_string(),
            "if loc0 == 1 {".to_string(),
            "    do1();".to_string(),
            "}".to_string(),
            "if loc0 == 2 {".to_string(),
            "    do2();".to_string(),
            "}".to_string(),
        ];
        HighLevelEmitter::rewrite_switch_statements(&mut statements);
        assert!(
            statements
                .iter()
                .any(|s| s.trim() == "loc5 = side_effect();"),
            "non-temp inter-case statement must survive: {statements:?}"
        );
    }

    #[test]
    fn switch_fold_preserves_side_effecting_temp_between_cases() {
        // Regression (adversarial): a temp capturing a discarded side-effecting
        // call between cases is NOT a case-value definition and must block the
        // fold so the call is not silently dropped.
        let mut statements = vec![
            "if loc0 == 0 {".to_string(),
            "    do0();".to_string(),
            "    return;".to_string(),
            "}".to_string(),
            "let t7 = Foo(arg);".to_string(),
            "if loc0 == 1 {".to_string(),
            "    do1();".to_string(),
            "    return;".to_string(),
            "}".to_string(),
            "if loc0 == 2 {".to_string(),
            "    do2();".to_string(),
            "    return;".to_string(),
            "}".to_string(),
        ];
        HighLevelEmitter::rewrite_switch_statements(&mut statements);
        assert!(
            statements.iter().any(|s| s.trim() == "let t7 = Foo(arg);"),
            "side-effecting temp must survive: {statements:?}"
        );
    }

    #[test]
    fn switch_fold_still_applies_to_consecutive_cases() {
        // Without an inter-case statement, three consecutive standalone-if cases
        // must still fold into a switch (the legitimate target pattern).
        let mut statements = vec![
            "if loc0 == 0 {".to_string(),
            "    do0();".to_string(),
            "}".to_string(),
            "if loc0 == 1 {".to_string(),
            "    do1();".to_string(),
            "}".to_string(),
            "if loc0 == 2 {".to_string(),
            "    do2();".to_string(),
            "}".to_string(),
        ];
        HighLevelEmitter::rewrite_switch_statements(&mut statements);
        assert!(
            statements.iter().any(|s| s.trim().starts_with("switch ")),
            "consecutive standalone-if cases should still fold to a switch: {statements:?}"
        );
    }
}
