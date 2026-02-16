//! Rewrite `if` / `else if` equality chains into `switch` statements.
//!
//! This pass is intentionally conservative: it only rewrites chains that
//! compare the same scrutinee expression against literal case values.
//!
//! Two patterns are recognized:
//! - `if/else if` chains (minimum 2 cases)
//! - Consecutive standalone `if` blocks comparing the same variable (minimum 3 cases)

use super::super::HighLevelEmitter;
use super::util::{extract_any_if_condition, is_else_if_open, is_else_open, is_if_open};

impl HighLevelEmitter {
    /// Rewrite eligible `if` / `else if` chains into `switch` blocks.
    pub(crate) fn rewrite_switch_statements(statements: &mut Vec<String>) {
        let mut index = 0usize;
        while index < statements.len() {
            if let Some((replacement, end)) = try_build_guarded_goto_switch(statements, index) {
                statements.splice(index..=end, replacement);
                index += 1;
                continue;
            }
            if let Some((replacement, end)) = try_build_switch(statements, index) {
                statements.splice(index..=end, replacement);
                index += 1;
                continue;
            }
            index += 1;
        }
    }
}

const MIN_GUARDED_GOTO_CASES: usize = 2;

fn try_build_guarded_goto_switch(
    statements: &[String],
    start: usize,
) -> Option<(Vec<String>, usize)> {
    let mut current_header = start;
    let mut labeled_cases: Vec<(String, String)> = Vec::new();
    let mut scrutinee: Option<String> = None;

    loop {
        let header_line = statements.get(current_header)?.trim();
        if let Some((condition, label)) = parse_inline_if_goto(header_line) {
            let resolved =
                resolve_condition_expression(statements, current_header, condition.as_str())?;
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
            labeled_cases.push((case_value, label));

            let next_header =
                find_next_guarded_header_after_case_prelude(statements, current_header + 1)?;
            current_header = next_header;
            continue;
        }
        break;
    }

    let header_line = statements.get(current_header)?.trim();
    if !is_if_open(header_line) {
        return None;
    }
    let condition = extract_any_if_condition(header_line)?;
    let resolved = resolve_condition_expression(statements, current_header, condition)?;
    let (next_scrutinee, case_token) = parse_case_sides(resolved.as_str())?;
    let final_case_value = resolve_case_value(statements, current_header, case_token)?;
    if !is_literal(final_case_value.as_str()) {
        return None;
    }
    if let Some(existing) = &scrutinee {
        if existing != &next_scrutinee {
            return None;
        }
    } else {
        scrutinee = Some(next_scrutinee);
    }

    let final_if_end = HighLevelEmitter::find_block_end(statements, current_header)?;
    let (default_label, label_blocks_start) =
        parse_guarded_switch_body_header(statements, current_header + 1, final_if_end)?;
    let label_bodies = collect_label_bodies(statements, label_blocks_start, final_if_end)?;

    let mut cases: Vec<(String, Vec<String>)> = Vec::new();
    for (case_value, label) in &labeled_cases {
        let body = label_bodies.get(label)?;
        if body.is_empty() {
            return None;
        }
        cases.push((case_value.clone(), body.clone()));
    }

    let (final_case_body, default_body, rewrite_end) = if let Some((else_header, else_end)) =
        find_else_block_after(statements, final_if_end + 1)
    {
        if let Some(default_label_index) = find_label_in_range(
            statements,
            else_header + 1,
            else_end,
            default_label.as_str(),
        ) {
            let final_case_body = statements
                .get(else_header + 1..default_label_index)
                .unwrap_or_default()
                .to_vec();
            if final_case_body.is_empty() {
                return None;
            }
            let default_body = statements
                .get(default_label_index + 1..else_end)
                .unwrap_or_default()
                .to_vec();
            if default_body.is_empty() {
                return None;
            }
            (final_case_body, default_body, else_end)
        } else {
            let final_case_body = statements
                .get(else_header + 1..else_end)
                .unwrap_or_default()
                .to_vec();
            if final_case_body.is_empty() {
                return None;
            }
            let default_label_index =
                find_label_after(statements, else_end + 1, default_label.as_str())?;
            let default_end = find_label_body_end(statements, default_label_index + 1);
            if default_end < default_label_index + 1 {
                return None;
            }
            let default_body = statements
                .get(default_label_index + 1..=default_end)
                .unwrap_or_default()
                .to_vec();
            if default_body.is_empty() {
                return None;
            }
            (final_case_body, default_body, default_end)
        }
    } else {
        let default_label_index =
            find_label_after(statements, final_if_end + 1, default_label.as_str())?;
        let final_case_body = statements
            .get(final_if_end + 1..default_label_index)
            .unwrap_or_default()
            .to_vec();
        if final_case_body.is_empty() {
            return None;
        }
        let default_end = find_label_body_end(statements, default_label_index + 1);
        if default_end < default_label_index + 1 {
            return None;
        }
        let default_body = statements
            .get(default_label_index + 1..=default_end)
            .unwrap_or_default()
            .to_vec();
        if default_body.is_empty() {
            return None;
        }
        (final_case_body, default_body, default_end)
    };

    cases.push((final_case_value, final_case_body));

    if cases.len() < MIN_GUARDED_GOTO_CASES {
        return None;
    }

    let mut seen = std::collections::BTreeSet::new();
    if !cases.iter().all(|(value, _)| seen.insert(value.clone())) {
        return None;
    }

    let mut output = Vec::new();
    output.push(format!("switch {} {{", scrutinee?));
    for (value, body) in &cases {
        output.push(format!("case {value} {{"));
        output.extend(body.iter().cloned());
        output.push("}".into());
    }
    output.push("default {".into());
    output.extend(default_body);
    output.push("}".into());
    output.push("}".into());

    Some((output, rewrite_end))
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
        let condition = extract_any_if_condition(header_line)?;
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
        if let Some(next_if_header) = find_next_if_after_case_prelude(statements, if_end + 1) {
            let next_if_line = statements[next_if_header].trim();
            if let Some(cond) = extract_any_if_condition(next_if_line) {
                if let Some(resolved) =
                    resolve_condition_expression(statements, next_if_header, cond)
                {
                    if let Some((peek_scrutinee, _)) = parse_case_sides(resolved.as_str()) {
                        if scrutinee.as_deref() == Some(peek_scrutinee.as_str()) {
                            current_header = next_if_header;
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
        if HighLevelEmitter::parse_assignment(statements[index].as_str()).is_some() {
            index += 1;
            continue;
        }
        return None;
    }
    None
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
        if HighLevelEmitter::parse_assignment(statements[index].as_str()).is_some() {
            index += 1;
            continue;
        }
        return None;
    }
    None
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
