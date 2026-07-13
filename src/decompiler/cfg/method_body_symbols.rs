use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::cfg::method_body::{
    MethodContext, MethodSymbolTypes, SymbolInfo, SymbolOrigin,
};
use crate::decompiler::cfg::ssa::{SsaForm, SsaVariable};
use crate::decompiler::cfg::EdgeKind;
use crate::decompiler::helpers::make_unique_identifier;

pub(super) fn allocate_source_symbols(
    context: &MethodContext,
    symbol_types: &MethodSymbolTypes,
    ssa: &SsaForm,
) -> (BTreeMap<String, String>, BTreeMap<String, SymbolInfo>) {
    let variables = ssa_variables(ssa);
    let mut source_names = context.source_names();
    let mut symbols = BTreeMap::new();
    let mut used = HashSet::new();

    for (index, name) in context.argument_names.iter().enumerate() {
        source_names.insert(format!("arg{index}"), name.clone());
        used.insert(name.clone());
        symbols.insert(
            name.clone(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(index),
                value_type: symbol_type(&symbol_types.parameters, index),
            },
        );
    }

    let mut argument_indices: BTreeSet<_> = (0..symbol_types.parameters.len()).collect();
    let mut local_indices: BTreeSet<_> = (0..symbol_types.locals.len()).collect();
    let mut static_indices: BTreeSet<_> = (0..symbol_types.statics.len()).collect();
    for variable in &variables {
        if let Some(index) = slot_index(&variable.base, "arg") {
            argument_indices.insert(index);
        } else if let Some(index) = slot_index(&variable.base, "loc") {
            local_indices.insert(index);
        } else if let Some(index) = slot_index(&variable.base, "static") {
            static_indices.insert(index);
        }
    }

    for index in argument_indices {
        if index < context.argument_names.len() {
            continue;
        }
        register_source_family(
            "arg",
            index,
            SymbolOrigin::Parameter(index),
            symbol_type(&symbol_types.parameters, index),
            &mut source_names,
            &mut symbols,
            &mut used,
        );
    }
    for index in local_indices {
        register_source_family(
            "loc",
            index,
            SymbolOrigin::Local(index),
            symbol_type(&symbol_types.locals, index),
            &mut source_names,
            &mut symbols,
            &mut used,
        );
    }
    for index in static_indices {
        register_source_family(
            "static",
            index,
            SymbolOrigin::Static(index),
            symbol_type(&symbol_types.statics, index),
            &mut source_names,
            &mut symbols,
            &mut used,
        );
    }

    for variable in variables {
        if variable.base == "?" || is_source_family(&variable.base) {
            continue;
        }
        let generated = format!("{}_{}", variable.base, variable.version);
        let emitted = make_unique_identifier(generated.clone(), &mut used);
        source_names.insert(generated, emitted.clone());
        symbols.entry(emitted).or_insert(SymbolInfo {
            origin: if variable.is_exception_payload() {
                SymbolOrigin::ExceptionPayload
            } else if is_stack_phi_base(&variable.base) {
                SymbolOrigin::Phi
            } else {
                SymbolOrigin::Temporary
            },
            value_type: if variable.is_exception_payload() {
                ValueType::Any
            } else {
                ValueType::Unknown
            },
        });
    }

    (source_names, symbols)
}

fn register_source_family(
    prefix: &str,
    index: usize,
    origin: SymbolOrigin,
    value_type: ValueType,
    source_names: &mut BTreeMap<String, String>,
    symbols: &mut BTreeMap<String, SymbolInfo>,
    used: &mut HashSet<String>,
) {
    let base = format!("{prefix}{index}");
    let emitted = make_unique_identifier(base.clone(), used);
    source_names.insert(base, emitted.clone());
    symbols.insert(emitted, SymbolInfo { origin, value_type });
}

fn ssa_variables(ssa: &SsaForm) -> BTreeSet<SsaVariable> {
    let mut variables: BTreeSet<_> = ssa
        .definitions
        .keys()
        .chain(ssa.uses.keys())
        .cloned()
        .collect();
    for block in ssa.blocks.values() {
        for phi in &block.phi_nodes {
            variables.insert(phi.target.clone());
            variables.extend(phi.operands.values().cloned());
        }
    }
    for edge in ssa
        .cfg
        .edges()
        .iter()
        .filter(|edge| edge.kind == EdgeKind::Exception)
    {
        variables.insert(SsaVariable::exception_payload(edge.to));
    }
    variables
}

fn slot_index(base: &str, prefix: &str) -> Option<usize> {
    let suffix = base.strip_prefix(prefix)?;
    (!suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| suffix.parse().ok())
        .flatten()
}

fn is_source_family(base: &str) -> bool {
    slot_index(base, "arg").is_some()
        || slot_index(base, "loc").is_some()
        || slot_index(base, "static").is_some()
}

fn is_stack_phi_base(base: &str) -> bool {
    base.strip_prefix('p').is_some_and(|suffix| {
        !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn symbol_type(types: &[ValueType], index: usize) -> ValueType {
    types.get(index).copied().unwrap_or(ValueType::Unknown)
}
