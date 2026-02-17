use super::super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(in super::super) fn find_increment_assignment(
        statements: &[String],
        start: usize,
        end: usize,
        var: &str,
    ) -> Option<(usize, Option<usize>, String)> {
        let mut index = end;
        while index > start {
            index -= 1;
            let line = statements[index].trim();
            if line.is_empty() || line.starts_with("//") || line == "}" {
                continue;
            }
            let assign = Self::parse_assignment(line)?;
            if assign.lhs != var {
                return None;
            }
            if assign.rhs.starts_with(var) {
                return Some((index, None, assign.full));
            }
            let prev_idx = Self::previous_code_line(statements, index)?;
            let prev_assign = Self::parse_assignment(&statements[prev_idx])?;
            if prev_assign.lhs == assign.rhs {
                let expr = format!("{} = {}", var, prev_assign.rhs);
                return Some((index, Some(prev_idx), expr));
            }
            if Self::contains_identifier(&assign.rhs, &prev_assign.lhs) {
                let replaced = Self::replace_identifier(
                    &assign.rhs,
                    &prev_assign.lhs,
                    prev_assign.rhs.as_str(),
                );
                let expr = format!("{} = {}", var, replaced);
                return Some((index, Some(prev_idx), expr));
            }
            return None;
        }
        None
    }

    pub(in super::super) fn should_inline_condition(rhs: &str) -> bool {
        matches!(rhs, "true" | "false")
            || rhs.contains(' ')
            || rhs.chars().any(|ch| {
                matches!(
                    ch,
                    '<' | '>' | '=' | '!' | '+' | '-' | '*' | '/' | '&' | '|'
                )
            })
    }
}
