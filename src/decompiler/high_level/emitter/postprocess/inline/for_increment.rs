use super::super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(in super::super::super) fn inline_for_increment_temps(statements: &mut [String]) {
        let mut index = 0;
        while index < statements.len() {
            let trimmed = statements[index].trim();
            if !trimmed.starts_with("for (") || !trimmed.ends_with('{') {
                index += 1;
                continue;
            }

            if let Some((init, condition, increment)) = Self::parse_for_parts(trimmed) {
                let mut depth = 1isize;
                let mut cursor = index + 1;
                while cursor < statements.len() && depth > 0 {
                    depth += Self::brace_delta(&statements[cursor]);
                    let line = statements[cursor].trim();
                    if depth <= 0 {
                        break;
                    }
                    if line.starts_with("let ") {
                        if let Some(assign) = Self::parse_assignment(line) {
                            let target = assign.lhs.clone();
                            let rhs = assign.rhs.clone();
                            if Self::contains_identifier(&increment, &target) {
                                // Only inline+delete the definition when the temp
                                // is used nowhere else (besides its own def and
                                // the for-header increment) AND its value is pure
                                // (no call). Otherwise clearing the definition
                                // would dangle a still-live reference, and moving
                                // a side-effecting RHS into the increment would
                                // change evaluation order.
                                let used_elsewhere =
                                    statements.iter().enumerate().any(|(i, stmt)| {
                                        i != cursor
                                            && i != index
                                            && Self::contains_identifier(stmt, &target)
                                    });
                                if used_elsewhere || rhs.contains('(') {
                                    cursor += 1;
                                    continue;
                                }
                                let replaced = Self::replace_identifier(&increment, &target, &rhs);
                                statements[index] =
                                    format!("for ({init}; {condition}; {replaced}) {{");
                                statements[cursor].clear();
                                break;
                            }
                        }
                    }
                    cursor += 1;
                }
            }
            index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::super::HighLevelEmitter;

    #[test]
    fn does_not_inline_for_increment_temp_used_elsewhere() {
        // Regression (adversarial): a temp used both in the for-increment and
        // by another body statement must NOT be inlined+deleted, or the other
        // use dangles and the side effect moves.
        let mut statements = vec![
            "for (let loc0 = 0; loc0 < 10; loc0 = loc0 + t5) {".to_string(),
            "    let t5 = step();".to_string(),
            "    log(t5);".to_string(),
            "}".to_string(),
        ];
        HighLevelEmitter::inline_for_increment_temps(&mut statements);
        assert!(
            statements.iter().any(|s| s.trim() == "let t5 = step();"),
            "still-used temp definition must survive: {statements:?}"
        );
        assert!(
            statements[0].contains("loc0 + t5"),
            "increment must not inline a non-single-use temp: {}",
            statements[0]
        );
    }

    #[test]
    fn inlines_pure_single_use_for_increment_temp() {
        // A pure, single-use increment temp is still inlined (the intended case).
        let mut statements = vec![
            "for (let loc0 = 0; loc0 < 10; loc0 = loc0 + t5) {".to_string(),
            "    let t5 = 1 + 1;".to_string(),
            "    body();".to_string(),
            "}".to_string(),
        ];
        HighLevelEmitter::inline_for_increment_temps(&mut statements);
        assert!(
            statements[0].contains("loc0 + 1 + 1") || statements[0].contains("loc0 + (1 + 1)"),
            "pure single-use temp should inline into the increment: {}",
            statements[0]
        );
    }
}
