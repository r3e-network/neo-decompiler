use super::super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(in super::super::super) fn inline_condition_temps(statements: &mut [String]) {
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
            } else if let Some(condition) = Self::extract_if_condition(&statements[index]) {
                if let Some(idx) = Self::previous_code_line(statements, index) {
                    if let Some(assign) = Self::parse_assignment(&statements[idx]) {
                        if assign.lhs == condition && Self::should_inline_condition(&assign.rhs) {
                            statements[index] = format!("if {} {{", assign.rhs);
                            statements[idx].clear();
                        }
                    }
                }
            }
            index += 1;
        }
    }
}
