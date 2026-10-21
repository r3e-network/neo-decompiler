//! The brace-balance invariant that the fuzz targets assert.
//!
//! Every fuzz target checks that the rendered output keeps its braces
//! balanced. The check has to ignore braces inside string literals and line
//! comments, because a decompiled contract may contain a byte string whose
//! payload happens to include a brace:
//!
//! ```text
//! let t_1 = unknown < "{f31\n\n\n{f3";
//! ```
//!
//! A naive character count reports that as an imbalance, and a fuzz campaign
//! that trusts a naive count spends its whole budget on false positives.
//!
//! The scan itself lives in the crate as `decompiler::ir::brace_balanced`, so
//! the fuzz targets, the emitter's post-processing passes, and this test all
//! use one implementation and cannot drift apart. This test pins the rules
//! down, including against the exact output that a fuzz campaign produced
//! before the check learned to ignore string literals.

#![allow(clippy::unwrap_used)]

use neo_decompiler::decompiler::ir::{brace_balanced, brace_delta};

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
    // The literal closes at the escaped quote only after the backslash, so
    // the brace after it is still inside the literal.
    assert_eq!(brace_delta("let t = \"a\\\"{b\";"), 0);
}

#[test]
fn balances_multiline_output() {
    assert!(brace_balanced("if (x) {\n  y();\n}"));
    assert!(!brace_balanced("if (x) {\n  y();\n"));
    assert!(brace_balanced("let t = \"{f31{f3\";\nreturn t;"));
}

/// The exact output that a fuzz campaign produced before the check learned to
/// ignore string literals. It is valid output, and the naive count called it
/// an imbalance.
#[test]
fn accepts_the_byte_string_that_contains_braces() {
    let rendered = "let t_1 = unknown < \"{f31\\n\\n\\n{f3\";\n// return/throw/abort at BlockId(0)";
    assert!(brace_balanced(rendered));
    // The naive count would have called this an imbalance, which is exactly
    // the false positive the shared scan removes.
    assert_ne!(
        rendered.matches('{').count(),
        rendered.matches('}').count(),
        "the fixture must still contain an unequal naive count"
    );
}
