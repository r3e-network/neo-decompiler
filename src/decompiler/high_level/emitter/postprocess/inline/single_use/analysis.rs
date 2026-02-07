use std::collections::{HashMap, HashSet};

use super::super::super::super::HighLevelEmitter;
use super::{util, InlineCandidate};

pub(super) fn collect_candidates(statements: &[String]) -> Vec<InlineCandidate> {
    // var -> (def_line, rhs)
    let mut definitions: HashMap<String, (usize, String)> = HashMap::new();
    let mut use_counts: HashMap<String, usize> = HashMap::new();
    let mut reassigned: HashSet<String> = HashSet::new();

    for (idx, line) in statements.iter().enumerate() {
        let trimmed = line.trim();
        let known: Vec<String> = definitions.keys().cloned().collect();

        if let Some(assign) = HighLevelEmitter::parse_assignment(trimmed) {
            // Count uses in RHS.
            for var in &known {
                if HighLevelEmitter::contains_identifier(&assign.rhs, var.as_str()) {
                    *use_counts.entry(var.clone()).or_insert(0) += 1;
                }
            }

            // Track only real temps (tN) as candidates for inlining.
            if !util::is_temp_identifier(assign.lhs.as_str()) {
                continue;
            }

            if trimmed.starts_with("let ") {
                // New definition.
                if definitions.contains_key(&assign.lhs) {
                    // Variable redefined, mark as reassigned.
                    reassigned.insert(assign.lhs.clone());
                } else {
                    definitions.insert(assign.lhs.clone(), (idx, assign.rhs.clone()));
                }
            } else {
                // Reassignment.
                reassigned.insert(assign.lhs.clone());
            }
        } else {
            // Count uses in other statements.
            for var in &known {
                if HighLevelEmitter::contains_identifier(trimmed, var.as_str()) {
                    *use_counts.entry(var.clone()).or_insert(0) += 1;
                }
            }
        }
    }

    let mut candidates = Vec::new();
    for (name, (def_line, rhs)) in &definitions {
        let count = use_counts.get(name).copied().unwrap_or(0);
        if count == 1 && !reassigned.contains(name) && util::is_safe_to_inline(rhs) {
            candidates.push(InlineCandidate {
                name: name.clone(),
                def_line: *def_line,
                rhs: rhs.clone(),
            });
        }
    }

    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.def_line));
    candidates
}
