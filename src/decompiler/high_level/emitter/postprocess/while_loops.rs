use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    /// Converts `goto label_X; do { ... label_X: ... } while (COND);` into
    /// `while (COND) { ... }` — recovering while-loop semantics from the
    /// compiler's forward-JMP-to-condition pattern.
    pub(crate) fn rewrite_goto_do_while(statements: &mut Vec<String>) {
        let mut index = 0;
        while index < statements.len() {
            let trimmed = statements[index].trim().to_string();

            // Match: goto label_0xXXXX;
            let Some(label) = trimmed
                .strip_prefix("goto ")
                .and_then(|s| s.strip_suffix(';'))
                .map(|s| s.trim().to_string())
            else {
                index += 1;
                continue;
            };

            // Next non-empty/comment line must be `do {`
            let Some(do_idx) = Self::next_code_line(statements, index) else {
                index += 1;
                continue;
            };
            if statements[do_idx].trim() != "do {" {
                index += 1;
                continue;
            }

            // Find the matching `} while (COND);`
            let Some(end_idx) = Self::find_block_end(statements, do_idx) else {
                index += 1;
                continue;
            };
            let end_trimmed = statements[end_idx].trim().to_string();
            let Some(condition) = end_trimmed
                .strip_prefix("} while (")
                .or_else(|| end_trimmed.strip_prefix("} while !("))
                .and_then(|s| s.strip_suffix(");"))
            else {
                index += 1;
                continue;
            };
            // Reconstruct the full condition including negation if present.
            let condition = if end_trimmed.starts_with("} while !(") {
                format!("!({condition})")
            } else {
                condition.to_string()
            };

            // Find `label_X:` inside the do-while body
            let label_line = format!("{label}:");
            let Some(label_idx) =
                (do_idx + 1..end_idx).find(|&i| statements[i].trim() == label_line)
            else {
                index += 1;
                continue;
            };

            // Collect non-empty, non-comment lines between label and } while
            let setup_lines: Vec<usize> = (label_idx + 1..end_idx)
                .filter(|&i| {
                    let t = statements[i].trim();
                    !t.is_empty() && !t.starts_with("//")
                })
                .collect();

            // Transform: remove goto, label; convert do→while, } while→}
            statements[index].clear(); // remove goto
            statements[do_idx] = format!("while {condition} {{");
            statements[label_idx].clear(); // remove label

            if setup_lines.is_empty() {
                // Pattern 1: no condition setup — clean while conversion
                statements[end_idx] = "}".to_string();
            } else {
                // Pattern 2: condition setup exists — duplicate before loop
                let setup_copies: Vec<String> =
                    setup_lines.iter().map(|&i| statements[i].clone()).collect();
                // Insert setup copies before the while line
                for (j, line) in setup_copies.into_iter().enumerate() {
                    statements.insert(do_idx + j, line);
                }
                // Indices shifted by number of inserted lines
                let shift = setup_lines.len();
                statements[end_idx + shift] = "}".to_string();
            }

            index += 1;
        }
    }

    /// Converts `goto label_X;` at end of switch cases to `break;` when
    /// `label_X:` appears immediately after the switch block.
    pub(crate) fn rewrite_switch_break_gotos(statements: &mut [String]) {
        let mut index = 0;
        while index < statements.len() {
            if !statements[index].trim().starts_with("switch ") {
                index += 1;
                continue;
            }
            let Some(end) = Self::find_block_end(statements, index) else {
                index += 1;
                continue;
            };
            // Check if next code line after switch `}` is a label
            let Some(label_idx) = Self::next_code_line(statements, end) else {
                index += 1;
                continue;
            };
            let label_trimmed = statements[label_idx].trim().to_string();
            let Some(label) = label_trimmed.strip_suffix(':') else {
                index += 1;
                continue;
            };
            if !label.starts_with("label_") {
                index += 1;
                continue;
            }
            // Replace matching gotos inside the switch with break
            let goto_target = format!("goto {label};");
            for i in index + 1..end {
                if statements[i].trim() == goto_target {
                    let indent =
                        &statements[i][..statements[i].len() - statements[i].trim_start().len()];
                    statements[i] = format!("{indent}break;");
                }
            }
            index = end + 1;
        }
    }

    /// Converts `label_X: <setup> if COND { <body> goto label_X; <phi> }` into
    /// `<setup> while COND { <body> <phi> <setup> }` — recovering while-loop
    /// semantics from backward unconditional JMPs inside if-blocks.
    pub(crate) fn rewrite_if_goto_to_while(statements: &mut Vec<String>) {
        let mut index = 0;
        while index < statements.len() {
            let trimmed = statements[index].trim().to_string();

            // Match: label_0xXXXX:
            let Some(label) = trimmed.strip_suffix(':') else {
                index += 1;
                continue;
            };
            if !label.starts_with("label_") {
                index += 1;
                continue;
            }

            // Find next `if ... {` after the label
            let Some(if_idx) = Self::next_if_line(statements, index) else {
                index += 1;
                continue;
            };

            // Find the matching `}`
            let Some(end_idx) = Self::find_block_end(statements, if_idx) else {
                index += 1;
                continue;
            };
            if statements[end_idx].trim() != "}" {
                index += 1;
                continue;
            }

            // Find `goto label_X;` inside the if-block
            let goto_target = format!("goto {label};");
            let Some(goto_idx) =
                (if_idx + 1..end_idx).find(|&i| statements[i].trim() == goto_target)
            else {
                index += 1;
                continue;
            };

            // Collect setup lines (non-empty, non-comment) between label and if
            let setup_lines: Vec<String> = (index + 1..if_idx)
                .filter(|&i| {
                    let t = statements[i].trim();
                    !t.is_empty() && !t.starts_with("//")
                })
                .map(|i| statements[i].clone())
                .collect();

            // Transform: remove label, change if→while, remove goto,
            // append setup copies at end of loop body
            statements[index].clear(); // remove label
            let if_line = statements[if_idx].trim().to_string();
            statements[if_idx] = if_line.replacen("if ", "while ", 1);
            statements[goto_idx].clear(); // remove goto

            // Insert setup copies before closing `}`
            if !setup_lines.is_empty() {
                for (j, line) in setup_lines.into_iter().enumerate() {
                    statements.insert(end_idx + j, line);
                }
            }

            index += 1;
        }
    }

    /// Recognises the `label_X: ... goto label_X;` shape with no other
    /// references to `label_X` and rewrites it to a `loop { ... }` block.
    /// This is the canonical pattern for an unconditional infinite loop
    /// produced by the Neo C# compiler; lifting it removes both the label
    /// and the goto, leaving idiomatic source.
    pub(crate) fn rewrite_label_goto_to_loop(statements: &mut [String]) {
        let mut index = 0;
        while index < statements.len() {
            let trimmed = statements[index].trim().to_string();

            // Match: `label_0xXXXX:`
            let Some(label) = trimmed.strip_suffix(':') else {
                index += 1;
                continue;
            };
            if !label.starts_with("label_") {
                index += 1;
                continue;
            }
            let label = label.to_string();

            // Find the matching `goto label_X;` at the same brace depth.
            let goto_target = format!("goto {label};");
            let mut depth = 0i32;
            let mut goto_idx = None;
            for (j, stmt) in statements.iter().enumerate().skip(index + 1) {
                let t = stmt.trim();
                if t == goto_target && depth == 0 {
                    goto_idx = Some(j);
                    break;
                }
                if t.ends_with('{') {
                    depth += 1;
                }
                if t == "}" || t.starts_with("} ") {
                    depth -= 1;
                    if depth < 0 {
                        // Exited the enclosing block before finding the goto.
                        break;
                    }
                }
            }
            let Some(goto_idx) = goto_idx else {
                index += 1;
                continue;
            };

            // Bail if any other reference to the label appears anywhere —
            // a second goto means the label is a structured-jump target,
            // not just a back-edge for an infinite loop, and lifting would
            // change semantics.
            let occurrences = statements
                .iter()
                .enumerate()
                .filter(|(i, stmt)| {
                    *i != index
                        && *i != goto_idx
                        && (stmt.trim() == format!("{label}:") || stmt.contains(&goto_target))
                })
                .count();
            if occurrences > 0 {
                index += 1;
                continue;
            }

            // Preserve indentation from the original label line so the
            // loop block stays aligned with surrounding code.
            let label_indent_len = statements[index].len() - statements[index].trim_start().len();
            let label_indent = statements[index][..label_indent_len].to_string();
            let goto_indent_len = statements[goto_idx].len() - statements[goto_idx].trim_start().len();
            let goto_indent = statements[goto_idx][..goto_indent_len].to_string();

            statements[index] = format!("{label_indent}loop {{");
            statements[goto_idx] = format!("{goto_indent}}}");
            index += 1;
        }
    }

    /// Removes `goto label_X;` when the very next code line is `label_X:`.
    pub(crate) fn eliminate_fallthrough_gotos(statements: &mut [String]) {
        let mut index = 0;
        while index < statements.len() {
            let trimmed = statements[index].trim().to_string();
            let Some(label) = trimmed
                .strip_prefix("goto ")
                .and_then(|s| s.strip_suffix(';'))
                .map(|s| s.trim().to_string())
            else {
                index += 1;
                continue;
            };

            if let Some(next) = Self::next_code_line(statements, index) {
                if statements[next].trim() == format!("{label}:") {
                    statements[index].clear();
                }
            }
            index += 1;
        }
    }

    fn next_if_line(statements: &[String], start: usize) -> Option<usize> {
        (start + 1..statements.len()).find(|&i| {
            let t = statements[i].trim();
            t.starts_with("if ") && t.ends_with('{')
        })
    }

    fn next_code_line(statements: &[String], start: usize) -> Option<usize> {
        (start + 1..statements.len()).find(|&i| {
            let t = statements[i].trim();
            !t.is_empty() && !t.starts_with("//")
        })
    }
}
