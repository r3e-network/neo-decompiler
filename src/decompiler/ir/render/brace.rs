//! Brace scanning for rendered output.
//!
//! Rendered C# and high-level text contain braces that do not open or close a
//! block: a byte string may carry a `{` in its payload, and a trailing
//! comment may contain a stray brace. Any tool that reasons about the block
//! structure of rendered output -- the post-processing passes that rewrite
//! `if`/`else` chains, and the fuzz targets that assert the output stays
//! well formed -- has to count only the braces that are really structural.
//!
//! The scan tracks the active quote character, honours backslash escapes,
//! and stops at a `//` comment, which is the same rule the emitter's own
//! block-boundary search applies.

/// Return the net brace delta of a line, ignoring braces inside string
/// literals and line comments.
///
/// A positive result means the line opens more blocks than it closes.
#[must_use]
pub fn brace_delta(line: &str) -> isize {
    let mut delta = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if let Some(expected) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == expected {
                quote = None;
            }
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'/') {
            break;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '{' => delta += 1,
            '}' => delta -= 1,
            _ => {}
        }
    }
    delta
}

/// Whether the rendered text keeps its braces balanced.
///
/// The text is scanned line by line with [`brace_delta`], so braces inside
/// string literals and trailing comments are not counted.
#[must_use]
pub fn brace_balanced(text: &str) -> bool {
    text.lines().map(brace_delta).sum::<isize>() == 0
}

#[cfg(test)]
mod tests {
    use super::{brace_balanced, brace_delta};

    #[test]
    fn counts_plain_braces() {
        assert_eq!(brace_delta("if (x) {"), 1);
        assert_eq!(brace_delta("}"), -1);
        assert_eq!(brace_delta("no braces"), 0);
    }

    #[test]
    fn ignores_braces_inside_string_literals() {
        assert_eq!(brace_delta("let t = \"{f31{f3\";"), 0);
        assert_eq!(brace_delta("let t = '}';"), 0);
    }

    #[test]
    fn ignores_braces_inside_line_comments() {
        assert_eq!(brace_delta("// a stray { brace"), 0);
    }

    #[test]
    fn honours_escaped_quotes() {
        // The literal closes at the escaped quote only after the backslash,
        // so the brace after it is still inside the literal.
        assert_eq!(brace_delta("let t = \"a\\\"{b\";"), 0);
    }

    #[test]
    fn balances_multiline_output() {
        assert!(brace_balanced("if (x) {\n  y();\n}"));
        assert!(!brace_balanced("if (x) {\n  y();\n"));
        assert!(brace_balanced("let t = \"{f31{f3\";\nreturn t;"));
    }
}
