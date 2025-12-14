use super::super::super::HighLevelEmitter;

mod analysis;
mod rewrite;
mod util;

#[derive(Debug, Clone)]
struct InlineCandidate {
    name: String,
    def_line: usize,
    rhs: String,
}

impl HighLevelEmitter {
    /// Inline single-use temporary variables.
    ///
    /// This pass identifies variables that are:
    /// 1. Defined with `let var = expr;`
    /// 2. Used exactly once in subsequent code
    /// 3. Not reassigned
    ///
    /// Such variables are inlined at their use site and the definition is removed.
    ///
    /// Note: This pass is available but disabled by default as it can be too
    /// aggressive for some use cases. Enable selectively when needed.
    pub(in super::super::super) fn inline_single_use_temps(statements: &mut [String]) {
        let candidates = analysis::collect_candidates(statements);
        rewrite::apply_inlining(statements, candidates);
    }
}

#[cfg(test)]
mod tests;
