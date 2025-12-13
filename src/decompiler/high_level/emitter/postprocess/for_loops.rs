use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(crate) fn rewrite_for_loops(statements: &mut [String]) {
        let mut index = 0;
        while index < statements.len() {
            let Some(condition) = Self::extract_while_condition(&statements[index]) else {
                index += 1;
                continue;
            };
            let Some(end) = Self::find_block_end(statements, index) else {
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
}
