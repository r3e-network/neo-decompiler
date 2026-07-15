use super::super::super::HighLevelEmitter;

fn is_numeric_or_simple_literal(value: &str) -> bool {
    let value = value.trim();
    if value == "true" || value == "false" || value == "null" {
        return true;
    }
    let digits = value.strip_prefix('-').unwrap_or(value);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

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
            // Compound forms emitted before the for-pass (`loc0 += 1`, `loc0++`).
            if let Some(rest) = line
                .strip_prefix(var)
                .and_then(|s| s.strip_prefix(" += "))
                .and_then(|s| s.strip_suffix(';'))
            {
                let rest = rest.trim();
                if !rest.is_empty() {
                    return Some((index, None, format!("{var} += {rest}")));
                }
            }
            if let Some(rest) = line
                .strip_prefix(var)
                .and_then(|s| s.strip_prefix(" -= "))
                .and_then(|s| s.strip_suffix(';'))
            {
                let rest = rest.trim();
                if !rest.is_empty() {
                    return Some((index, None, format!("{var} -= {rest}")));
                }
            }
            if line == format!("{var}++;") || line == format!("++{var};") {
                return Some((index, None, format!("{var}++")));
            }
            if line == format!("{var}--;") || line == format!("--{var};") {
                return Some((index, None, format!("{var}--")));
            }

            let assign = Self::parse_assignment(line)?;
            if assign.lhs != var {
                return None;
            }
            // Prefer folding a pure constant temp (`let t3 = 1; loc0 = loc0 + t3`)
            // into the increment before accepting the raw assignment form.
            if let Some(prev_idx) = Self::previous_code_line(statements, index) {
                if let Some(prev_assign) = Self::parse_assignment(&statements[prev_idx]) {
                    if prev_assign.lhs == assign.rhs {
                        let expr = format!("{} = {}", var, prev_assign.rhs);
                        return Some((index, Some(prev_idx), expr));
                    }
                    if Self::contains_identifier(&assign.rhs, &prev_assign.lhs)
                        && is_numeric_or_simple_literal(&prev_assign.rhs)
                    {
                        let replaced = Self::replace_identifier(
                            &assign.rhs,
                            &prev_assign.lhs,
                            prev_assign.rhs.as_str(),
                        );
                        let expr = format!("{} = {}", var, replaced);
                        return Some((index, Some(prev_idx), expr));
                    }
                }
            }
            if assign.rhs.starts_with(var) {
                return Some((index, None, assign.full));
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
