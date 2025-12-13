use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(in super::super) fn inline_condition_temps(statements: &mut [String]) {
        let mut index = 0;
        while index < statements.len() {
            if let Some(condition) = Self::extract_while_condition(&statements[index]) {
                if let Some(idx) = Self::previous_code_line(statements, index) {
                    if let Some(assign) = Self::parse_assignment(&statements[idx]) {
                        if assign.lhs == condition && Self::should_inline_condition(&assign.rhs) {
                            statements[index] = format!("while {} {{", assign.rhs);
                            statements[idx].clear();
                        }
                    }
                }
            } else if let Some((init, condition, increment)) =
                Self::parse_for_parts(&statements[index])
            {
                if let Some(idx) = Self::previous_code_line(statements, index) {
                    if let Some(assign) = Self::parse_assignment(&statements[idx]) {
                        if assign.lhs == condition && Self::should_inline_condition(&assign.rhs) {
                            statements[index] =
                                format!("for ({init}; {}; {increment}) {{", assign.rhs);
                            statements[idx].clear();
                        }
                    }
                }
            }
            index += 1;
        }
    }

    pub(in super::super) fn inline_for_increment_temps(statements: &mut [String]) {
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
                    let delta = Self::brace_delta(&statements[cursor]);
                    depth += delta;
                    let line = statements[cursor].trim();
                    if depth <= 0 {
                        break;
                    }
                    if line.starts_with("let ") {
                        if let Some(assign) = Self::parse_assignment(line) {
                            let target = &assign.lhs;
                            if increment.contains(target) {
                                let replaced =
                                    increment.replace(target.as_str(), assign.rhs.as_str());
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
