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
        // The scan itself lives next to the renderers so the fuzz targets
        // and the post-processing passes count exactly the same braces.
        crate::decompiler::ir::brace_delta(line)
    }
}

#[cfg(test)]
mod tests {
    use super::HighLevelEmitter;

    fn brace_delta(line: &str) -> isize {
        HighLevelEmitter::brace_delta(line)
    }

    fn find_end(lines: &[&str]) -> Option<usize> {
        let stmts: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
        HighLevelEmitter::find_block_end(&stmts, 0)
    }

    #[test]
    fn brace_delta_ignores_line_comments() {
        assert_eq!(brace_delta("    // {"), 0);
        assert_eq!(brace_delta("    // }"), 0);
        assert_eq!(brace_delta("    // \" {"), 0);
        assert_eq!(brace_delta("    // ' }"), 0);
    }

    #[test]
    fn brace_delta_counts_code_before_trailing_comments() {
        assert_eq!(brace_delta("if true { // }"), 1);
        assert_eq!(brace_delta("} // {"), -1);
        assert_eq!(brace_delta("log(\"//\"); } // {"), -1);
        assert_eq!(brace_delta("log('//'); { // }"), 1);
        assert_eq!(brace_delta("log(\"escaped \\\" // {\"); } // {"), -1);
    }

    #[test]
    fn brace_delta_ignores_string_literal_braces() {
        assert_eq!(brace_delta("    log(\"{\");"), 0);
        assert_eq!(brace_delta("    log(\"}\");"), 0);
    }

    #[test]
    fn find_block_end_ignores_comment_braces() {
        for comment in ["    // {", "    // }", "    // \" {", "    // ' }"] {
            let lines = ["if true {", comment, "    work();", "}", "after();"];
            assert_eq!(find_end(&lines), Some(3), "comment: {comment:?}");
        }
    }

    #[test]
    fn find_block_end_ignores_string_literal_braces() {
        let lines = [
            "if true {",
            "    log(\"{\");",
            "    log(\"}\");",
            "    work();",
            "}",
            "after();",
        ];
        assert_eq!(find_end(&lines), Some(4));
    }

    #[test]
    fn find_block_end_nested_with_comment_braces() {
        let lines = [
            "if true {",
            "    if flag {",
            "        // }",
            "        work();",
            "    }",
            "    afterInner();",
            "}",
            "after();",
        ];
        assert_eq!(find_end(&lines), Some(6));
    }
}
