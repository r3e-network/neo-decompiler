use super::super::HighLevelEmitter;

fn is_numeric_literal(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    let digits = value.strip_prefix('-').unwrap_or(value);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

fn condition_mentions_ident(condition: &str, ident: &str) -> bool {
    // Word-boundary style check so `loc10` does not match `loc1`.
    condition
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|token| token == ident)
}

impl HighLevelEmitter {
    /// Lift `loop { let x = c; if cond(x) { … update x … } }` into
    /// `let x = c; while cond(x) { … }` so the subsequent for-loop pass can
    /// promote counting shapes. Matches LoopIf-class back-edges that re-enter
    /// the initializer and would otherwise leave a defeated condition inside
    /// an infinite loop.
    pub(crate) fn rewrite_header_init_loops(statements: &mut [String]) {
        let mut index = 0;
        while index < statements.len() {
            if statements[index].trim() != "loop {" {
                index += 1;
                continue;
            }
            let Some(loop_end) = Self::find_block_end(statements, index) else {
                index += 1;
                continue;
            };

            let code: Vec<usize> = (index + 1..loop_end)
                .filter(|&i| {
                    let t = statements[i].trim();
                    !t.is_empty() && !t.starts_with("//")
                })
                .collect();
            if code.len() < 3 {
                index += 1;
                continue;
            }

            // Allow pure constant temp lets between the induction init and the
            // `if` (e.g. `let t1 = 3;` feeding `if loc0 < t1`) when those temps
            // were not yet inlined.
            let Some(init_idx) = code.iter().copied().find(|&i| {
                Self::parse_assignment(&statements[i]).is_some_and(|a| {
                    is_numeric_literal(&a.rhs)
                        && (a.lhs.starts_with("loc")
                            || a.lhs.starts_with("arg")
                            || a.lhs.starts_with("static"))
                })
            }) else {
                index += 1;
                continue;
            };
            let init_pos = code.iter().position(|&i| i == init_idx).unwrap_or(0);
            let Some(if_idx) = code[init_pos + 1..]
                .iter()
                .copied()
                .find(|&i| statements[i].trim().starts_with("if "))
            else {
                index += 1;
                continue;
            };
            // Everything between init and if must be pure constant lets.
            let between_ok = code[init_pos + 1..]
                .iter()
                .copied()
                .take_while(|&i| i != if_idx)
                .all(|i| {
                    Self::parse_assignment(&statements[i])
                        .is_some_and(|a| is_numeric_literal(&a.rhs))
                });
            if !between_ok {
                index += 1;
                continue;
            }
            let Some(init) = Self::parse_assignment(&statements[init_idx]) else {
                index += 1;
                continue;
            };
            let Some(condition) = Self::extract_if_condition(&statements[if_idx]) else {
                index += 1;
                continue;
            };
            if !condition_mentions_ident(&condition, &init.lhs) {
                index += 1;
                continue;
            }
            let Some(if_end) = Self::find_block_end(statements, if_idx) else {
                index += 1;
                continue;
            };
            // Require the if (including its closer) to be the last code in the loop.
            if code.last().copied() != Some(if_end) {
                index += 1;
                continue;
            }

            // Body of the if must update the induction variable.
            let body_updates = (if_idx + 1..if_end).any(|i| {
                Self::parse_assignment(&statements[i]).is_some_and(|a| a.lhs == init.lhs)
                    || statements[i]
                        .trim()
                        .starts_with(&format!("{} +=", init.lhs))
                    || statements[i]
                        .trim()
                        .starts_with(&format!("{} -=", init.lhs))
                    || statements[i].trim().starts_with(&format!("{}++", init.lhs))
                    || statements[i].trim().starts_with(&format!("{}--", init.lhs))
            });
            if !body_updates {
                index += 1;
                continue;
            }

            let indent_len = statements[index].len() - statements[index].trim_start().len();
            let indent = statements[index][..indent_len].to_string();
            let init_line = statements[init_idx].clone();
            // Hoist init before loop, convert loop+if into while(cond).
            statements[index] = init_line;
            statements[init_idx].clear();
            statements[if_idx] = format!("{indent}while {condition} {{");
            // Drop the outer loop's closer; the if closer becomes the while closer.
            statements[loop_end].clear();
            // Re-indent is already fine; continue after rewritten while.
            index += 1;
        }
    }

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
            for stmt in statements.iter_mut().take(end).skip(index + 1) {
                if stmt.trim() == goto_target {
                    let indent = stmt[..stmt.len() - stmt.trim_start().len()].to_string();
                    *stmt = format!("{indent}break;");
                }
            }
            index = end + 1;
        }
    }

    /// Converts `label_X: <setup> if COND { <body> goto label_X; <phi> }` into
    /// `<setup> while COND { <body> <phi> <setup> }` — recovering while-loop
    /// semantics from backward unconditional JMPs inside if-blocks.
    pub(crate) fn rewrite_if_goto_to_while(statements: &mut Vec<String>) {
        // The pass can only succeed when a matching `goto label_X;` exists
        // (checked at lines below inside the if-block). Pre-collect those once
        // so labels with no corresponding goto are skipped in O(1) instead of
        // each running an unbounded forward `next_if_line` scan to the tail of
        // the vector. Without this guard the pass is O(labels × N): a crafted
        // in-cap NEF that emits many `label_X:` lines with no following `if`
        // drives it quadratic — a decompiler-hang DoS.
        let goto_targets: std::collections::HashSet<String> = statements
            .iter()
            .map(|s| s.trim())
            .filter(|t| t.starts_with("goto label_") && t.ends_with(';'))
            .map(str::to_string)
            .collect();
        if goto_targets.is_empty() {
            return;
        }

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

            // Fast-skip: with no `goto label_X;` anywhere, the search below can
            // only fail, so skip the unbounded `next_if_line` scan entirely.
            if !goto_targets.contains(format!("goto {label};").as_str()) {
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
        // A `label_X:` can only fold into a `loop { }` when a matching STANDALONE
        // `goto label_X;` exists. Pre-collect those lines once so labels without
        // one are skipped in O(1) instead of each scanning the whole tail of the
        // vector. Without this the pass is O(labels × N): a crafted in-cap NEF
        // that emits many guarded gotos (e.g. crossing JMPIF chains, whose gotos
        // render as inline `if c { goto X; }` and never match the standalone
        // form) drives it quadratic — a decompiler-hang DoS.
        let standalone_gotos: std::collections::HashSet<String> = statements
            .iter()
            .map(|stmt| stmt.trim())
            .filter(|trimmed| trimmed.starts_with("goto label_") && trimmed.ends_with(';'))
            .map(str::to_string)
            .collect();
        if standalone_gotos.is_empty() {
            return;
        }

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
            // Fast-skip: with no standalone goto for this label, the forward
            // search below can only fail, so skip the scan entirely.
            if !standalone_gotos.contains(&goto_target) {
                index += 1;
                continue;
            }
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
            // change semantics. (`label_decl` is hoisted out of the scan so it
            // is allocated once rather than per statement, and `.any()`
            // short-circuits on the first extra reference.)
            let label_decl = format!("{label}:");
            let has_other_reference = statements.iter().enumerate().any(|(i, stmt)| {
                i != index
                    && i != goto_idx
                    && (stmt.trim() == label_decl || stmt.contains(&goto_target))
            });
            if has_other_reference {
                index += 1;
                continue;
            }

            // Preserve indentation from the original label line so the
            // loop block stays aligned with surrounding code.
            let label_indent_len = statements[index].len() - statements[index].trim_start().len();
            let label_indent = statements[index][..label_indent_len].to_string();
            let goto_indent_len =
                statements[goto_idx].len() - statements[goto_idx].trim_start().len();
            let goto_indent = statements[goto_idx][..goto_indent_len].to_string();

            statements[index] = format!("{label_indent}loop {{");
            statements[goto_idx] = format!("{goto_indent}}}");
            index += 1;
        }
    }

    /// Removes `goto label_X;` (or the try-context `leave label_X;` form)
    /// when the resume target sits at the very next code line *or* one or
    /// more close-braces past it.
    ///
    /// `leave` is the high-level encoding of `ENDTRY <target>` — semantically
    /// "exit the try block, run finally, resume at target". When the resume
    /// target is the next instruction, the lowered C#/pseudocode would
    /// auto-execute finally on any try exit anyway, so the explicit transfer
    /// is dead code that only adds visual noise. The same logic applies when
    /// a `goto`/`leave` is the last statement of a block (e.g. a catch body)
    /// whose closing `}` is immediately followed by the target label.
    pub(crate) fn eliminate_fallthrough_gotos(statements: &mut [String]) {
        let mut index = 0;
        while index < statements.len() {
            let trimmed = statements[index].trim().to_string();
            let Some(label) = trimmed
                .strip_prefix("goto ")
                .or_else(|| trimmed.strip_prefix("leave "))
                .and_then(|s| s.strip_suffix(';'))
                .map(|s| s.trim().to_string())
            else {
                index += 1;
                continue;
            };

            let label_line = format!("{label}:");
            // Walk forward past blank/comment/close-brace lines to find the
            // next executable statement. If it is the matching label, the
            // transfer is dead — control would have reached the label
            // through structural fall-out anyway.
            let mut probe = index + 1;
            while probe < statements.len() {
                let t = statements[probe].trim();
                if t.is_empty() || t.starts_with("//") || t == "}" {
                    probe += 1;
                    continue;
                }
                if t == label_line {
                    statements[index].clear();
                }
                break;
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
