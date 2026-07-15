use super::*;

/// Second pass over call edges: resolve CALLA targets that load their function
/// pointer from an argument slot (LDARG N) by tracing back through callers.
pub(crate) fn resolve_ldarg_calla_targets(
    instructions: &[Instruction],
    edges: &mut [CallEdge],
    table: &MethodTable,
    methods: &mut BTreeMap<usize, MethodRef>,
) {
    // Build offset -> instruction-index map.
    let offset_to_index: BTreeMap<usize, usize> = instructions
        .iter()
        .enumerate()
        .map(|(i, instr)| (instr.offset, i))
        .collect();

    // Collect unresolved CALLA sites preceded by LDARG.
    // NOTE: edge.caller.offset may be inaccurate for internal helpers discovered
    // during the first pass (the MethodTable was built before those methods were
    // found). Use the `methods` map - which now contains all first-pass
    // discoveries - to find the true containing method for each CALLA.
    let mut sites: Vec<(usize, u8, usize)> = Vec::new(); // (edge_index, arg_index, method_offset)
    for (edge_idx, edge) in edges.iter().enumerate() {
        if edge.opcode != "CALLA" || !matches!(edge.target, CallTarget::Indirect { .. }) {
            continue;
        }
        let Some(&calla_idx) = offset_to_index.get(&edge.call_offset) else {
            continue;
        };
        if let Some(arg_idx) = calla_ldarg_index(instructions, calla_idx) {
            // Find the actual method containing this CALLA by looking up the
            // largest method offset <= the CALLA offset in the methods map.
            let actual_method_offset = methods
                .range(..=edge.call_offset)
                .next_back()
                .map(|(&offset, _)| offset)
                .unwrap_or(edge.caller.offset);
            sites.push((edge_idx, arg_idx, actual_method_offset));
        }
    }

    if sites.is_empty() {
        return;
    }

    loop {
        let mut callers_by_target: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for edge in edges.iter() {
            if let CallTarget::Internal { method } = &edge.target {
                if edge.opcode == "CALL" || edge.opcode == "CALL_L" || edge.opcode == "CALLA" {
                    callers_by_target
                        .entry(method.offset)
                        .or_default()
                        .push(edge.call_offset);
                }
            }
        }

        let mut progress = false;
        for (edge_idx, arg_idx, method_offset) in &sites {
            if !matches!(edges[*edge_idx].target, CallTarget::Indirect { .. }) {
                continue;
            }

            let mut visited = BTreeSet::new();
            let resolved = resolve_argument_target_recursive(
                instructions,
                &offset_to_index,
                &callers_by_target,
                methods,
                *method_offset,
                *arg_idx,
                &mut visited,
            );

            if let Some(target) = resolved.filter(|target| offset_to_index.contains_key(target)) {
                let callee = table.resolve_internal_target(target);
                methods.insert(callee.offset, callee.clone());
                edges[*edge_idx].target = CallTarget::Internal { method: callee };
                progress = true;
            }
        }

        if !progress {
            break;
        }
    }
}

fn resolve_argument_target_recursive(
    instructions: &[Instruction],
    offset_to_index: &BTreeMap<usize, usize>,
    callers_by_target: &BTreeMap<usize, Vec<usize>>,
    methods: &BTreeMap<usize, MethodRef>,
    method_offset: usize,
    arg_index: u8,
    visited: &mut BTreeSet<(usize, u8)>,
) -> Option<usize> {
    if !visited.insert((method_offset, arg_index)) {
        return None;
    }

    let call_sites = callers_by_target.get(&method_offset)?;
    let callee_arg_count =
        initslot_arg_count_at(instructions, method_offset).unwrap_or(arg_index as usize + 1);

    for &call_offset in call_sites {
        let &call_idx = offset_to_index.get(&call_offset)?;
        match trace_call_arg_source(instructions, call_idx, arg_index, callee_arg_count) {
            Some(CallArgSource::Target(target)) => return Some(target),
            Some(CallArgSource::PassThrough(next_arg)) => {
                let caller_method_offset = methods
                    .range(..=call_offset)
                    .next_back()
                    .map(|(&offset, _)| offset)
                    .unwrap_or(call_offset);
                if let Some(target) = resolve_argument_target_recursive(
                    instructions,
                    offset_to_index,
                    callers_by_target,
                    methods,
                    caller_method_offset,
                    next_arg,
                    visited,
                ) {
                    return Some(target);
                }
            }
            None => {}
        }
    }

    None
}
