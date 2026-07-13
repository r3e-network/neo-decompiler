//! Temporary-value cleanup passes.

use super::super::super::HighLevelEmitter;

impl HighLevelEmitter {
    /// Eliminates identity assignments `let tN = tM;` by substituting tN -> tM
    /// in all subsequent code.
    pub(crate) fn eliminate_identity_temps(statements: &mut [String]) {
        let mut first_seen: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (line_index, stmt) in statements.iter().enumerate() {
            for tok in Self::temp_tokens(stmt) {
                first_seen.entry(tok).or_insert(line_index);
            }
        }

        let mut index = 0;
        while index < statements.len() {
            let trimmed = statements[index].trim();
            let Some(assign) = Self::parse_assignment(trimmed) else {
                index += 1;
                continue;
            };
            if !trimmed.starts_with("let ")
                || !Self::is_temp_ident(&assign.lhs)
                || !Self::is_temp_ident(&assign.rhs)
            {
                index += 1;
                continue;
            }
            if assign.lhs == assign.rhs {
                statements[index].clear();
                index += 1;
                continue;
            }
            let lhs = assign.lhs.clone();
            let rhs = assign.rhs.clone();
            if first_seen.get(&lhs).is_some_and(|&first| first < index) {
                index += 1;
                continue;
            }
            for stmt in statements.iter_mut().skip(index + 1) {
                if Self::contains_identifier(stmt, &lhs) {
                    *stmt = Self::replace_identifier(stmt, &lhs, &rhs);
                }
            }
            statements[index].clear();
            index += 1;
        }
    }

    /// Collapses an immediately consumed temporary into its assignment or return.
    pub(crate) fn collapse_temp_into_store(statements: &mut [String]) {
        let mut temp_line_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for stmt in statements.iter() {
            for tok in Self::temp_tokens(stmt) {
                *temp_line_counts.entry(tok).or_insert(0) += 1;
            }
        }

        let mut index = 0;
        while index + 1 < statements.len() {
            let trimmed = statements[index].trim();
            let Some(a1) = Self::parse_assignment(trimmed) else {
                index += 1;
                continue;
            };
            if !trimmed.starts_with("let ") || !Self::is_temp_ident(&a1.lhs) {
                index += 1;
                continue;
            }
            let mut next = index + 1;
            while next < statements.len() {
                let t = statements[next].trim();
                if !t.is_empty() && !t.starts_with("//") {
                    break;
                }
                next += 1;
            }
            if next >= statements.len() {
                index += 1;
                continue;
            }
            let trimmed_next = statements[next].trim();
            let temp = &a1.lhs;
            if let Some(a2) = Self::parse_assignment(trimmed_next) {
                if a2.rhs == *temp && temp_line_counts.get(temp).copied().unwrap_or(0) <= 2 {
                    let indent = &statements[next][..statements[next].len() - trimmed_next.len()];
                    let prefix = if trimmed_next.starts_with("let ") {
                        "let "
                    } else {
                        ""
                    };
                    statements[next] = format!("{indent}{prefix}{} = {};", a2.lhs, a1.rhs);
                    statements[index].clear();
                    index = next + 1;
                    continue;
                }
            }
            if trimmed_next == format!("return {};", temp)
                && temp_line_counts.get(temp).copied().unwrap_or(0) <= 2
            {
                let indent = &statements[next][..statements[next].len() - trimmed_next.len()];
                statements[next] = format!("{indent}return {};", a1.rhs);
                statements[index].clear();
                index = next + 1;
                continue;
            }
            index += 1;
        }
    }

    /// Removes `let tN = <pure_value>;` lines whose lhs is never referenced.
    /// Pure values are literals, simple identifiers, or known-pure expressions.
    pub(crate) fn eliminate_dead_temps(statements: &mut [String]) {
        let mut index = 0;
        while index < statements.len() {
            let trimmed = statements[index].trim();
            if !trimmed.starts_with("let ") {
                index += 1;
                continue;
            }
            let Some(assign) = Self::parse_assignment(trimmed) else {
                index += 1;
                continue;
            };
            if !Self::is_temp_ident(&assign.lhs) || !Self::is_pure_rhs(&assign.rhs) {
                index += 1;
                continue;
            }
            let lhs = &assign.lhs;
            let used_anywhere = statements
                .iter()
                .enumerate()
                .any(|(i, stmt)| i != index && Self::contains_identifier(stmt, lhs));
            if !used_anywhere {
                statements[index].clear();
            }
            index += 1;
        }
    }
}
