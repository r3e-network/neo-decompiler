use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(crate) fn rewrite_for_loops(statements: &mut [String]) {
        // Precompute each opener's matching closer in one O(n) pass instead of
        // re-scanning from every `while` header (which is O(statements^2) when
        // many headers never close). The pass only clears non-brace lines
        // (init/increment/temp) and replaces `while … {` with `for (…) {`, so
        // the brace structure — and therefore these indices — stay valid.
        let block_ends = Self::precompute_block_ends(statements);
        let mut index = 0;
        while index < statements.len() {
            let Some(condition) = Self::extract_while_condition(&statements[index]) else {
                index += 1;
                continue;
            };
            let Some(end) = block_ends.get(index).copied().flatten() else {
                index += 1;
                continue;
            };
            let Some(init_idx) = Self::find_initializer_index(statements, index) else {
                index += 1;
                continue;
            };
            let Some(init_assignment) = Self::parse_assignment(&statements[init_idx]) else {
                index += 1;
                continue;
            };
            let Some((increment_idx, temp_idx, increment_expr)) =
                Self::find_increment_assignment(statements, index, end, &init_assignment.lhs)
            else {
                index += 1;
                continue;
            };

            statements[index] = format!(
                "for ({}; {}; {}) {{",
                init_assignment.full, condition, increment_expr
            );
            statements[init_idx].clear();
            statements[increment_idx].clear();
            if let Some(temp_idx) = temp_idx {
                statements[temp_idx].clear();
            }
            index += 1;
        }
    }

    /// Map every block-opener line to the index of its matching closer in a
    /// single O(n) pass, mirroring [`Self::find_block_end`]'s cumulative
    /// `brace_delta` accounting.
    ///
    /// A brace-balance stack records, for each `{`, the line that brings its
    /// level back to closed. For the single-`{` while/for headers this pass
    /// queries, the recorded index is exactly what re-scanning from the header
    /// with `find_block_end` would return — including when nested or balanced
    /// multi-brace lines appear in between — but it is computed once rather than
    /// once per header, removing the quadratic blow-up on many unclosed headers.
    fn precompute_block_ends(statements: &[String]) -> Vec<Option<usize>> {
        let mut ends = vec![None; statements.len()];
        let mut open_stack: Vec<usize> = Vec::new();
        for (line_idx, line) in statements.iter().enumerate() {
            let delta = Self::brace_delta(line);
            if delta > 0 {
                for _ in 0..delta {
                    open_stack.push(line_idx);
                }
            } else if delta < 0 {
                for _ in 0..-delta {
                    if let Some(opener) = open_stack.pop() {
                        ends[opener] = Some(line_idx);
                    }
                }
            }
        }
        ends
    }
}

#[cfg(test)]
mod tests {
    use super::HighLevelEmitter;

    /// The precomputed block-end for every single-`{` opener must equal what
    /// re-scanning from that line with `find_block_end` returns. This is the
    /// correctness contract that lets `rewrite_for_loops` drop the per-header
    /// rescan (the O(statements^2) → O(statements) fix).
    fn assert_matches_find_block_end(lines: &[&str]) {
        let stmts: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
        let ends = HighLevelEmitter::precompute_block_ends(&stmts);
        for (idx, line) in stmts.iter().enumerate() {
            // Only single-`{`, zero-`}` openers are queried by the for-loop
            // pass (the `while … {` / `for (…) {` headers); the precompute is
            // contracted to match `find_block_end` exactly for those.
            if line.matches('{').count() == 1 && !line.contains('}') {
                assert_eq!(
                    ends[idx],
                    HighLevelEmitter::find_block_end(&stmts, idx),
                    "mismatch at line {idx}: {line:?} in {lines:?}",
                );
            }
        }
    }

    #[test]
    fn precompute_block_ends_matches_find_block_end_simple() {
        assert_matches_find_block_end(&["while (t0 < 10) {", "    t0 = t0 + 1;", "}"]);
    }

    #[test]
    fn precompute_block_ends_matches_find_block_end_nested() {
        assert_matches_find_block_end(&[
            "while (t0 < 10) {",
            "    while (t1 < 5) {",
            "        t1 = t1 + 1;",
            "    }",
            "    t0 = t0 + 1;",
            "}",
        ]);
    }

    #[test]
    fn precompute_block_ends_matches_find_block_end_sequential_and_else() {
        assert_matches_find_block_end(&[
            "while (t0 < 10) {",
            "    if t0 {",
            "    } else {",
            "        t0 = t0 + 1;",
            "    }",
            "}",
            "while (t1 < 3) {",
            "    t1 = t1 + 1;",
            "}",
        ]);
    }

    #[test]
    fn precompute_block_ends_matches_find_block_end_unclosed_headers() {
        // The DoS shape: many headers that never close. Each must resolve to
        // `None` (no matching closer), exactly as `find_block_end` would.
        let lines: Vec<String> = (0..64).map(|i| format!("while (t{i} < 10) {{")).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        assert_matches_find_block_end(&refs);
        let ends = HighLevelEmitter::precompute_block_ends(&lines);
        assert!(
            ends.iter().all(Option::is_none),
            "unclosed headers must have no matching closer",
        );
    }

    #[test]
    fn precompute_block_ends_matches_find_block_end_balanced_inline_braces() {
        // Intermediate balanced-brace lines (delta 0) must not perturb the
        // matching of the enclosing single-`{` header.
        assert_matches_find_block_end(&[
            "while (t0 < 10) {",
            "    if t0 { goto label_3; }",
            "    t0 = t0 + 1;",
            "}",
        ]);
    }
}
