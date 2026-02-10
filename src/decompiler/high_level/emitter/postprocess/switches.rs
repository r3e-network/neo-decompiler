//! Rewrite `if` / `else if` equality chains into `switch` statements.
//!
//! This pass is intentionally conservative: it only rewrites chains that
//! compare the same scrutinee expression against literal case values.
//!
//! Two patterns are recognized:
//! - `if/else if` chains (minimum 2 cases)
//! - Consecutive standalone `if` blocks comparing the same variable (minimum 3 cases)

use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    /// Rewrite eligible `if` / `else if` chains into `switch` blocks.
    pub(crate) fn rewrite_switch_statements(statements: &mut Vec<String>) {
        let mut index = 0usize;
        while index < statements.len() {
            let Some((replacement, end)) = try_build_switch(statements, index) else {
                index += 1;
                continue;
            };

            statements.splice(index..=end, replacement);
            index += 1;
        }
    }
}

fn try_build_switch(statements: &[String], start: usize) -> Option<(Vec<String>, usize)> {
    let header = statements.get(start)?.trim();
    if !is_if_open(header) {
        return None;
    }

    let mut cases: Vec<(String, Vec<String>)> = Vec::new();
    let mut default_body: Option<Vec<String>> = None;
    let mut overall_end = start;
    let mut has_else_link = false;

    let mut current_header = start;
    let mut scrutinee: Option<String> = None;

    loop {
        let header_line = statements.get(current_header)?.trim();
        let condition = extract_if_condition(header_line)?;
        let resolved = resolve_condition_expression(statements, current_header, condition)?;
        let (next_scrutinee, case_token) = parse_case_sides(resolved.as_str())?;

        let case_value = resolve_case_value(statements, current_header, case_token)?;
        if !is_literal(case_value.as_str()) {
            return None;
        }

        if let Some(existing) = &scrutinee {
            if existing != &next_scrutinee {
                return None;
            }
        } else {
            scrutinee = Some(next_scrutinee);
        }

        let (body, if_end) = extract_block_body(statements, current_header)?;
        overall_end = overall_end.max(if_end);
        cases.push((case_value, body));

        let (trivia, next_header) = collect_trivia(statements, if_end + 1);
        if next_header >= statements.len() {
            break;
        }

        let next_line = statements[next_header].trim();
        if is_else_if_open(next_line) {
            has_else_link = true;
            if let Some((_, last_body)) = cases.last_mut() {
                last_body.extend(trivia);
            }
            current_header = next_header;
            continue;
        }

        if is_else_open(next_line) {
            has_else_link = true;
            let else_end = HighLevelEmitter::find_block_end(statements, next_header)?;
            overall_end = overall_end.max(else_end);

            // Try to flatten an `else { <if-chain> }` into an `else if`.
            if let Some(inner_start) = find_first_if_in_range(statements, next_header + 1, else_end)
            {
                let inner_chain_end = end_of_if_chain(statements, inner_start)?;
                if inner_chain_end < else_end {
                    let (inner_trivia, after_inner) =
                        collect_trivia(statements, inner_chain_end + 1);
                    let only_trivia_left = after_inner == else_end;
                    if only_trivia_left {
                        if let Some((_, last_body)) = cases.last_mut() {
                            last_body.extend(trivia);
                            last_body.extend(inner_trivia);
                        }
                        current_header = inner_start;
                        overall_end = overall_end.max(else_end);
                        continue;
                    }
                }
            }

            if let Some((_, last_body)) = cases.last_mut() {
                last_body.extend(trivia);
            }

            default_body = Some(
                statements
                    .get(next_header + 1..else_end)
                    .unwrap_or_default()
                    .to_vec(),
            );
            break;
        }

        // Consecutive standalone `if` comparing the same scrutinee.
        if is_if_open(next_line) {
            if let Some(cond) = extract_if_condition(next_line) {
                if let Some(resolved) = resolve_condition_expression(statements, next_header, cond)
                {
                    if let Some((peek_scrutinee, _)) = parse_case_sides(resolved.as_str()) {
                        if scrutinee.as_deref() == Some(peek_scrutinee.as_str()) {
                            current_header = next_header;
                            continue;
                        }
                    }
                }
            }
        }

        break;
    }

    let scrutinee = scrutinee?;

    // Require at least 2 cases for `if/else if` chains (unambiguous pattern)
    // and at least 3 for consecutive standalone `if` blocks (conservative).
    let min_cases = if has_else_link { 2 } else { 3 };
    if cases.len() < min_cases {
        return None;
    }

    // Ensure case values are unique for readability.
    {
        let mut seen = std::collections::BTreeSet::new();
        if !cases.iter().all(|(value, _)| seen.insert(value.clone())) {
            return None;
        }
    }

    let mut output = Vec::new();
    output.push(format!("switch {scrutinee} {{"));
    for (value, body) in &cases {
        output.push(format!("case {value} {{"));
        output.extend(body.iter().cloned());
        output.push("}".into());
    }
    if let Some(body) = default_body {
        output.push("default {".into());
        output.extend(body);
        output.push("}".into());
    }
    output.push("}".into());

    Some((output, overall_end))
}

fn is_if_open(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("if ") && trimmed.ends_with(" {")
}

fn is_else_if_open(line: &str) -> bool {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix("} ").unwrap_or(trimmed);
    trimmed.starts_with("else if ") && trimmed.ends_with(" {")
}

fn extract_if_condition(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix("} ").unwrap_or(trimmed);
    let without_prefix = trimmed
        .strip_prefix("if ")
        .or_else(|| trimmed.strip_prefix("else if "))?;
    without_prefix.strip_suffix(" {")
}

fn is_else_open(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "else {" || trimmed == "} else {"
}

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
    let prev = HighLevelEmitter::previous_code_line(statements, header_index)?;
    let assign = HighLevelEmitter::parse_assignment(statements[prev].as_str())?;
    (assign.lhs == condition.trim()).then_some(assign.rhs)
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
    if matches!(value, "true" | "false" | "null") {
        return true;
    }
    if value.starts_with("0x") && value.len() > 2 {
        return value[2..].chars().all(|ch| ch.is_ascii_hexdigit());
    }
    value.parse::<i64>().is_ok()
}

fn is_temp(value: &str) -> bool {
    let value = value.trim();
    value.starts_with('t') && value[1..].chars().all(|ch| ch.is_ascii_digit())
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

fn end_of_if_chain(statements: &[String], start: usize) -> Option<usize> {
    let if_end = HighLevelEmitter::find_block_end(statements, start)?;
    let (_, next) = collect_trivia(statements, if_end + 1);
    if next < statements.len() && is_else_open(statements[next].trim()) {
        return HighLevelEmitter::find_block_end(statements, next);
    }
    Some(if_end)
}
