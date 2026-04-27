//! Final-pass formatting cleanup: join a standalone close-brace line
//! with the next chain-continuation header (`else {`, `else if cond {`,
//! `catch (...) {`, `finally {`) so the rendered output uses the K&R
//! single-line form `} else {` instead of the multi-line `}\nelse {`.
//!
//! Earlier passes emit the close-brace and the chain header as separate
//! statements (the emitter pushes them at different offsets and lets
//! the renderer indent them). For end-user output, joining them on a
//! single line is the conventional formatting for both C# (the C#
//! emit's source-of-truth language) and idiomatic pseudo-code, and
//! matches the JS port's rendering byte-for-byte for the same NEFs.

use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    /// Walk `statements` and replace each `}\n<chain>` pair (where
    /// `<chain>` is `else {`, `else if cond {`, `catch (...) {`, or
    /// `finally {`) with a single joined `} <chain>` line. Preserves
    /// the indentation of the chain-continuation line (the
    /// close-brace's indent is dropped) so the resulting block opener
    /// sits at the depth of the body that follows.
    pub(crate) fn join_close_brace_with_chain(statements: &mut [String]) {
        let mut index = 0;
        while index + 1 < statements.len() {
            // Skip blank lines / pure-comment lines between the brace
            // and the chain header — we want to find the next
            // semantically meaningful line.
            if statements[index].trim() != "}" {
                index += 1;
                continue;
            }
            let mut next = index + 1;
            while next < statements.len() {
                let t = statements[next].trim();
                if !t.is_empty() && !t.starts_with("//") {
                    break;
                }
                next += 1;
            }
            if next >= statements.len() {
                index += 1;
                continue;
            }
            let chain = statements[next].trim();
            let is_chain = chain == "else {"
                || chain.starts_with("else if ")
                || chain == "finally {"
                || chain.starts_with("catch ")
                || chain.starts_with("catch(");
            if !is_chain {
                index += 1;
                continue;
            }
            let indent = &statements[next][..statements[next].len() - chain.len()];
            statements[next] = format!("{indent}}} {chain}");
            statements[index].clear();
            index = next + 1;
        }
    }
}
