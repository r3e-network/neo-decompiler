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
                            let target = &assign.lhs;
                            if Self::contains_identifier(&increment, target) {
                                let replaced = Self::replace_identifier(
                                    &increment,
                                    target,
                                    assign.rhs.as_str(),
                                );
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
