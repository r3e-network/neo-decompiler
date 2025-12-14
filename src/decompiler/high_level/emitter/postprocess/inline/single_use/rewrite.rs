use super::super::super::super::HighLevelEmitter;
use super::{util, InlineCandidate};

pub(super) fn apply_inlining(statements: &mut [String], candidates: Vec<InlineCandidate>) {
    for candidate in candidates {
        let mut inlined = false;
        for statement in statements.iter_mut().skip(candidate.def_line + 1) {
            if !HighLevelEmitter::contains_identifier(statement, candidate.name.as_str()) {
                continue;
            }

            // Don't inline into control flow conditions (if, while, for) as it reduces readability,
            // unless the RHS is a trivial atom (literal/identifier).
            if util::is_control_flow_condition(statement)
                && !util::is_trivial_inline_rhs(candidate.rhs.as_str())
            {
                break;
            }

            let replacement = if util::needs_parens(candidate.rhs.as_str()) {
                format!("({})", candidate.rhs)
            } else {
                candidate.rhs.clone()
            };

            let updated = HighLevelEmitter::replace_identifier(
                statement,
                candidate.name.as_str(),
                &replacement,
            );
            if updated != statement.as_str() {
                *statement = updated;
                inlined = true;
                break;
            }
        }

        if inlined {
            statements[candidate.def_line].clear();
        }
    }
}
