use super::super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(in super::super) fn find_block_end(statements: &[String], start: usize) -> Option<usize> {
        let mut depth = Self::brace_delta(&statements[start]);
        let mut index = start + 1;
        while index < statements.len() {
            depth += Self::brace_delta(&statements[index]);
            if depth == 0 {
                return Some(index);
            }
            index += 1;
        }
        None
    }

    pub(in super::super) fn brace_delta(line: &str) -> isize {
        let openings = line.matches('{').count() as isize;
        let closings = line.matches('}').count() as isize;
        openings - closings
    }
}
