use super::super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(in super::super::super) fn inline_condition_temps(statements: &mut [String]) {
        let mut index = 0;
        while index < statements.len() {
            if let Some(condition) = Self::extract_while_condition(&statements[index]) {
                if let Some((idx, inlined)) =
                    Self::condition_inline_candidate(statements, index, &condition)
                {
                    statements[index] = format!("while {inlined} {{");
                    statements[idx].clear();
                }
            } else if let Some((init, condition, increment)) =
                Self::parse_for_parts(&statements[index])
            {
                if let Some((idx, inlined)) =
                    Self::condition_inline_candidate(statements, index, &condition)
                {
                    statements[index] = format!("for ({init}; {inlined}; {increment}) {{");
                    statements[idx].clear();
                }
            } else if let Some(condition) = Self::extract_if_condition(&statements[index]) {
                if let Some((idx, inlined)) =
                    Self::condition_inline_candidate(statements, index, &condition)
                {
                    statements[index] = format!("if {inlined} {{");
                    statements[idx].clear();
                }
            }
            index += 1;
        }
    }

    /// If the code line preceding `index` is `let <tmp> = <rhs>;` and the
    /// loop/if `condition` is either `<tmp>` or `!<tmp>` (with an inline-safe
    /// rhs), return the source line index plus the inlined condition text.
    ///
    /// A negated temp inlines to `!(<rhs>)` so the `!` binds the whole
    /// expression — without this the comparison stays hoisted into a
    /// loop-invariant temp computed once before the loop (`let t = i > 3; while
    /// !t {}`), which misrepresents a loop that the VM re-tests every iteration.
    /// Mirrors the JS port, which renders `!(i > 3)` inline.
    fn condition_inline_candidate(
        statements: &[String],
        index: usize,
        condition: &str,
    ) -> Option<(usize, String)> {
        let idx = Self::previous_code_line(statements, index)?;
        let assign = Self::parse_assignment(&statements[idx])?;
        if !Self::should_inline_condition(&assign.rhs) {
            return None;
        }
        if assign.lhs == condition {
            Some((idx, assign.rhs))
        } else if condition.strip_prefix('!').map(str::trim) == Some(assign.lhs.as_str()) {
            Some((idx, format!("!({})", assign.rhs)))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::super::HighLevelEmitter;

    #[test]
    fn inlines_negated_loop_condition_temp() {
        // A JMPIF-at-loop-top comparison materialises `let t = i > 3;` and a
        // `while !t {` header. The temp must inline as `!(i > 3)` so the loop
        // re-evaluates the comparison each iteration rather than testing a
        // loop-invariant temp computed once before the loop.
        let mut statements = vec![
            "let t2 = loc0 > 3;".to_string(),
            "while !t2 {".to_string(),
            "}".to_string(),
        ];
        HighLevelEmitter::inline_condition_temps(&mut statements);
        assert_eq!(statements[0], "", "the materialised temp must be removed");
        assert_eq!(statements[1], "while !(loc0 > 3) {");
    }

    #[test]
    fn inlines_negated_for_condition_temp() {
        let mut statements = vec![
            "let t2 = loc0 > 3;".to_string(),
            "for (let loc0 = 0; !t2; loc0 += 1) {".to_string(),
            "}".to_string(),
        ];
        HighLevelEmitter::inline_condition_temps(&mut statements);
        assert_eq!(statements[0], "");
        assert_eq!(
            statements[1],
            "for (let loc0 = 0; !(loc0 > 3); loc0 += 1) {"
        );
    }

    #[test]
    fn still_inlines_bare_condition_temp() {
        // Regression guard: the non-negated (JMPIFNOT) form must keep inlining
        // the bare comparison, unchanged by the negated-form handling.
        let mut statements = vec![
            "let t2 = loc0 > 3;".to_string(),
            "while t2 {".to_string(),
            "}".to_string(),
        ];
        HighLevelEmitter::inline_condition_temps(&mut statements);
        assert_eq!(statements[0], "");
        assert_eq!(statements[1], "while loc0 > 3 {");
    }
}
