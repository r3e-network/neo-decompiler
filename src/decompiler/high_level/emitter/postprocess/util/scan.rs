use super::super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(in super::super) fn find_initializer_index(
        statements: &[String],
        start: usize,
    ) -> Option<usize> {
        let mut index = start;
        while index > 0 {
            index -= 1;
            let line = statements[index].trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            if line == "}" || line.ends_with("{") {
                break;
            }
            if line.contains('=') && line.ends_with(';') {
                if let Some(assign) = Self::parse_assignment(line) {
                    if assign.lhs.starts_with("loc")
                        || assign.lhs.starts_with("arg")
                        || assign.lhs.starts_with("static")
                    {
                        return Some(index);
                    }
                }
            }
        }
        None
    }

    pub(in super::super) fn previous_code_line(
        statements: &[String],
        mut index: usize,
    ) -> Option<usize> {
        while index > 0 {
            index -= 1;
            let line = statements[index].trim();
            if line.is_empty() || line.starts_with("//") || line == "}" {
                continue;
            }
            return Some(index);
        }
        None
    }
}
