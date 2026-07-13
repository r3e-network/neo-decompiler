//! Guarded-goto switch rewriting.

use super::super::super::HighLevelEmitter;
use super::super::util::{extract_any_if_condition, is_if_open};
use super::{
    collect_label_bodies, find_else_block_after, find_label_after, find_label_body_end,
    find_label_in_range, find_next_guarded_header_after_case_prelude, is_literal, parse_case_sides,
    parse_guarded_switch_body_header, parse_inline_if_goto, resolve_case_value,
    resolve_condition_expression, MIN_GUARDED_GOTO_CASES,
};

pub(super) fn try_build_guarded_goto_switch(
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
            current_header =
                find_next_guarded_header_after_case_prelude(statements, current_header + 1)?;
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
            let default_body = statements
                .get(default_label_index + 1..else_end)
                .unwrap_or_default()
                .to_vec();
            if final_case_body.is_empty() || default_body.is_empty() {
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
    let mut output = vec![format!("switch {} {{", scrutinee?)];
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
