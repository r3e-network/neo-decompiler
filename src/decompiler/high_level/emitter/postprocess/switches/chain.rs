//! Equality-chain to switch rewriting.

use super::super::super::HighLevelEmitter;
use super::super::util::{extract_any_if_condition, is_else_if_open, is_else_open, is_if_open};
use super::{
    case_body_is_switch_safe, collect_trivia, end_of_if_chain, extract_block_body,
    find_first_if_in_range, find_next_if_after_case_prelude, parse_case_sides, resolve_case_value,
    resolve_condition_expression,
};

pub(super) fn try_build_switch(
    statements: &[String],
    start: usize,
) -> Option<(Vec<String>, usize)> {
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
        if !super::is_literal(case_value.as_str()) {
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
            if let Some(inner_start) = find_first_if_in_range(statements, next_header + 1, else_end)
            {
                let inner_chain_end = end_of_if_chain(statements, inner_start)?;
                if inner_chain_end < else_end {
                    let (inner_trivia, after_inner) =
                        collect_trivia(statements, inner_chain_end + 1);
                    if after_inner == else_end {
                        if let Some((_, last_body)) = cases.last_mut() {
                            last_body.extend(trivia);
                            last_body.extend(inner_trivia);
                        }
                        current_header = inner_start;
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

        if let Some(next_if_header) = find_next_if_after_case_prelude(statements, if_end + 1) {
            if let Some(cond) = extract_any_if_condition(statements[next_if_header].trim()) {
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
    let min_cases = if has_else_link { 2 } else { 3 };
    if cases.len() < min_cases {
        return None;
    }
    let mut seen = std::collections::BTreeSet::new();
    if !cases.iter().all(|(value, _)| seen.insert(value.clone())) {
        return None;
    }
    if !has_else_link
        && !cases
            .iter()
            .all(|(_, body)| case_body_is_switch_safe(body, &scrutinee))
    {
        return None;
    }

    let mut output = vec![format!("switch {scrutinee} {{")];
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
