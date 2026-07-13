use super::*;

pub(super) fn record_definition_facts(
    statements: &[SsaStmt],
    opcode: OpCode,
    state: &mut BuildPassState<'_>,
    return_shape: Option<CollectionShape>,
    seeded_facts: Option<CollectionShapeFacts>,
    static_index: Option<usize>,
) {
    for statement in statements {
        let SsaStmt::Assign { target, value } = statement else {
            continue;
        };
        let is_integer_literal = match value {
            SsaExpr::Literal(Literal::Int(_) | Literal::BigInt(_)) => {
                opcode_produces_integer_literal(opcode)
            }
            SsaExpr::Variable(source) => state
                .definition_facts
                .get(source)
                .is_some_and(|fact| fact.is_integer_literal),
            _ => false,
        };
        let inferred_facts = match value {
            SsaExpr::Array(elements) => CollectionShapeFacts {
                shape: Some(CollectionShape::Array(elements.len())),
                indexed: indexed_collection_shapes_for_elements(elements, state),
            },
            SsaExpr::Struct(elements) => CollectionShapeFacts {
                shape: Some(CollectionShape::Struct(elements.len())),
                indexed: indexed_collection_shapes_for_elements(elements, state),
            },
            SsaExpr::Variable(source) => {
                collection_shape_facts_for_variable_from_state(source, state)
            }
            SsaExpr::Index { base, index } => CollectionShapeFacts {
                shape: indexed_collection_shape_for_access(base, index, state),
                indexed: BTreeMap::new(),
            },
            SsaExpr::Call {
                target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Pickitem)),
                args,
            } => CollectionShapeFacts {
                shape: args.first().zip(args.get(1)).and_then(|(base, index)| {
                    indexed_collection_shape_for_access(base, index, state)
                }),
                indexed: BTreeMap::new(),
            },
            SsaExpr::Call { .. } => CollectionShapeFacts {
                shape: return_shape,
                indexed: BTreeMap::new(),
            },
            _ => CollectionShapeFacts::default(),
        };
        let has_reaching_definition = match value {
            SsaExpr::Variable(source) => state.definition_facts.contains_key(source),
            _ => false,
        };
        let collection_facts = if has_reaching_definition {
            inferred_facts
        } else {
            seeded_facts.clone().unwrap_or(inferred_facts)
        };
        let mut static_indexes = match value {
            SsaExpr::Variable(source) => {
                collection_fact_root(source, state.definition_facts, &mut BTreeSet::new())
                    .and_then(|root| state.definition_facts.get(&root))
                    .map(|fact| fact.static_indexes.clone())
                    .unwrap_or_default()
            }
            _ => BTreeSet::new(),
        };
        static_indexes.extend(static_index);
        let is_collection_root = static_index.is_some()
            || (!collection_facts.is_empty() && !matches!(value, SsaExpr::Variable(_)));
        state.definition_facts.insert(
            target.clone(),
            DefinitionFact {
                expression: value.clone(),
                is_integer_literal,
                collection_shape: collection_facts.shape,
                indexed_shapes: collection_facts.indexed,
                is_collection_root,
                static_indexes,
            },
        );
    }
}

pub(super) fn indexed_collection_shape_for_access(
    base: &SsaExpr,
    index: &SsaExpr,
    state: &BuildPassState<'_>,
) -> Option<CollectionShape> {
    let selected_index = match index {
        SsaExpr::Variable(variable) => {
            resolve_nonnegative_literal(variable, state.definition_facts, &mut BTreeSet::new())
        }
        SsaExpr::Literal(Literal::Int(value)) => usize::try_from(*value).ok(),
        _ => None,
    }?;
    collection_shape_facts_for_expression_from_state(base, state)
        .indexed
        .get(&selected_index)
        .copied()
}

pub(super) fn indexed_collection_shapes_for_elements(
    elements: &[SsaExpr],
    state: &BuildPassState<'_>,
) -> BTreeMap<usize, CollectionShape> {
    elements
        .iter()
        .enumerate()
        .filter_map(|(index, element)| {
            collection_shape_facts_for_expression_from_state(element, state)
                .shape
                .map(|shape| (index, shape))
        })
        .collect()
}

pub(super) fn opcode_produces_integer_literal(opcode: OpCode) -> bool {
    matches!(
        opcode,
        OpCode::PushM1
            | OpCode::Push0
            | OpCode::Push1
            | OpCode::Push2
            | OpCode::Push3
            | OpCode::Push4
            | OpCode::Push5
            | OpCode::Push6
            | OpCode::Push7
            | OpCode::Push8
            | OpCode::Push9
            | OpCode::Push10
            | OpCode::Push11
            | OpCode::Push12
            | OpCode::Push13
            | OpCode::Push14
            | OpCode::Push15
            | OpCode::Push16
            | OpCode::Pushint8
            | OpCode::Pushint16
            | OpCode::Pushint32
            | OpCode::Pushint64
            | OpCode::Pushint128
            | OpCode::Pushint256
            | OpCode::Depth
            | OpCode::Unpack
    )
}

pub(super) fn is_collection_fact(expression: &SsaExpr) -> bool {
    matches!(
        expression,
        SsaExpr::Array(_) | SsaExpr::Struct(_) | SsaExpr::Map(_)
    )
}

pub(super) fn resolve_collection_fact<'a>(
    variable: &SsaVariable,
    facts: &'a DefinitionFacts,
    invalidated_content_roots: &BTreeSet<SsaVariable>,
    invalidated_roots: &BTreeSet<SsaVariable>,
    visited: &mut BTreeSet<SsaVariable>,
) -> Option<&'a SsaExpr> {
    if invalidated_content_roots.contains(variable) || invalidated_roots.contains(variable) {
        return None;
    }
    if !visited.insert(variable.clone()) {
        return None;
    }
    let expression = &facts.get(variable)?.expression;
    match expression {
        expression if is_collection_fact(expression) => Some(expression),
        SsaExpr::Variable(source) => resolve_collection_fact(
            source,
            facts,
            invalidated_content_roots,
            invalidated_roots,
            visited,
        ),
        _ => None,
    }
}

pub(super) fn resolve_collection_shape(
    variable: &SsaVariable,
    facts: &DefinitionFacts,
    invalidated_roots: &BTreeSet<SsaVariable>,
    visited: &mut BTreeSet<SsaVariable>,
) -> Option<CollectionShape> {
    if invalidated_roots.contains(variable) || !visited.insert(variable.clone()) {
        return None;
    }
    let fact = facts.get(variable)?;
    match &fact.expression {
        SsaExpr::Variable(source) => {
            resolve_collection_shape(source, facts, invalidated_roots, visited)
        }
        _ => fact.collection_shape,
    }
}

pub(super) fn collection_shape_facts_for_variable(
    variable: &SsaVariable,
    facts: &DefinitionFacts,
    collection_state: &CollectionInvalidations,
) -> CollectionShapeFacts {
    collection_shape_facts_for_variable_parts(
        variable,
        facts,
        &collection_state.shapes,
        &collection_state.indexed_shapes,
    )
}

pub(super) fn collection_shape_facts_for_variable_from_state(
    variable: &SsaVariable,
    state: &BuildPassState<'_>,
) -> CollectionShapeFacts {
    collection_shape_facts_for_variable_parts(
        variable,
        state.definition_facts,
        state.invalidated_collection_roots,
        state.indexed_collection_shapes,
    )
}

pub(super) fn collection_shape_facts_for_variable_parts(
    variable: &SsaVariable,
    facts: &DefinitionFacts,
    invalidated_roots: &BTreeSet<SsaVariable>,
    indexed_shape_overrides: &BTreeMap<SsaVariable, BTreeMap<usize, CollectionShape>>,
) -> CollectionShapeFacts {
    let Some(root) = collection_fact_root(variable, facts, &mut BTreeSet::new()) else {
        return CollectionShapeFacts::default();
    };
    if invalidated_roots.contains(&root) {
        return CollectionShapeFacts::default();
    }
    let shape = resolve_collection_shape(variable, facts, invalidated_roots, &mut BTreeSet::new());
    let indexed = indexed_shape_overrides
        .get(&root)
        .cloned()
        .or_else(|| facts.get(&root).map(|fact| fact.indexed_shapes.clone()))
        .unwrap_or_default();
    CollectionShapeFacts { shape, indexed }
}

pub(super) fn collection_shape_facts_for_expression_from_state(
    expression: &SsaExpr,
    state: &BuildPassState<'_>,
) -> CollectionShapeFacts {
    match expression {
        SsaExpr::Array(elements) => CollectionShapeFacts {
            shape: Some(CollectionShape::Array(elements.len())),
            indexed: indexed_collection_shapes_for_elements(elements, state),
        },
        SsaExpr::Struct(elements) => CollectionShapeFacts {
            shape: Some(CollectionShape::Struct(elements.len())),
            indexed: indexed_collection_shapes_for_elements(elements, state),
        },
        SsaExpr::Variable(variable) => {
            collection_shape_facts_for_variable_from_state(variable, state)
        }
        _ => CollectionShapeFacts::default(),
    }
}

pub(super) fn collection_shape_for_expression(
    expression: &SsaExpr,
    facts: &DefinitionFacts,
    invalidated_roots: &BTreeSet<SsaVariable>,
) -> Option<CollectionShape> {
    match expression {
        SsaExpr::Array(elements) => Some(CollectionShape::Array(elements.len())),
        SsaExpr::Struct(elements) => Some(CollectionShape::Struct(elements.len())),
        SsaExpr::Variable(variable) => {
            resolve_collection_shape(variable, facts, invalidated_roots, &mut BTreeSet::new())
        }
        _ => None,
    }
}

pub(super) fn unanimous_collection_shape(
    return_shapes: &[Option<CollectionShape>],
) -> Option<CollectionShape> {
    let first = return_shapes.first().copied().flatten()?;
    return_shapes
        .iter()
        .all(|shape| *shape == Some(first))
        .then_some(first)
}

pub(super) fn unanimous_argument_field_writes(
    return_writes: &[Vec<BTreeMap<usize, CollectionShape>>],
) -> Vec<BTreeMap<usize, CollectionShape>> {
    let Some(first) = return_writes.first() else {
        return Vec::new();
    };
    (0..first.len())
        .map(|argument_index| {
            let mut unanimous = first[argument_index].clone();
            unanimous.retain(|field, shape| {
                return_writes.iter().all(|writes| {
                    writes
                        .get(argument_index)
                        .and_then(|fields| fields.get(field))
                        == Some(shape)
                })
            });
            unanimous
        })
        .collect()
}

pub(super) fn collection_fact_root(
    variable: &SsaVariable,
    facts: &DefinitionFacts,
    visited: &mut BTreeSet<SsaVariable>,
) -> Option<SsaVariable> {
    if !visited.insert(variable.clone()) {
        return None;
    }
    let fact = facts.get(variable)?;
    if !fact.static_indexes.is_empty() {
        return Some(variable.clone());
    }
    match &fact.expression {
        SsaExpr::Variable(source) => collection_fact_root(source, facts, visited),
        expression
            if is_collection_fact(expression)
                || fact.collection_shape.is_some()
                || fact.is_collection_root =>
        {
            Some(variable.clone())
        }
        _ => None,
    }
}

pub(super) fn mark_static_collection_alias(
    variable: &SsaVariable,
    index: usize,
    facts: &mut DefinitionFacts,
) {
    let Some(root) = collection_fact_root(variable, facts, &mut BTreeSet::new()) else {
        return;
    };
    if let Some(fact) = facts.get_mut(&root) {
        fact.static_indexes.insert(index);
    }
}

pub(super) fn record_static_alias_mutation(
    receiver: &SsaVariable,
    preserves_shape: bool,
    provisional: bool,
    state: &mut BuildPassState<'_>,
) {
    let Some(root) = collection_fact_root(receiver, state.definition_facts, &mut BTreeSet::new())
    else {
        return;
    };
    let static_indexes = state
        .definition_facts
        .get(&root)
        .map(|fact| fact.static_indexes.clone())
        .unwrap_or_default();
    if static_indexes.is_empty() {
        return;
    }
    state
        .invalidated_static_collection_shapes
        .extend(static_indexes.iter().copied());
    let facts = preserves_shape
        .then(|| collection_shape_facts_for_variable_from_state(receiver, state))
        .filter(|facts| !facts.is_empty());
    state
        .static_collection_writes
        .extend(
            static_indexes
                .into_iter()
                .map(|index| StaticCollectionWrite {
                    index,
                    facts: facts.clone(),
                    is_null: false,
                    provisional,
                }),
        );
}

pub(super) fn record_static_call_argument_effects(
    contract: &super::super::context::CallContract,
    argument_roots: &[Option<SsaVariable>],
    argument_effects: &[CollectionArgumentEffect],
    state: &mut BuildPassState<'_>,
) {
    let internal = matches!(&contract.target, SemanticCallTarget::Internal { .. });
    for (root, effect) in argument_roots.iter().zip(argument_effects) {
        let Some(root) = root else {
            continue;
        };
        match effect {
            CollectionArgumentEffect::ReadOnly => {}
            CollectionArgumentEffect::PreservesShape => {
                record_static_alias_mutation(root, true, false, state);
            }
            CollectionArgumentEffect::Unknown => {
                record_static_alias_mutation(root, false, internal, state);
            }
        }
    }
}

pub(super) fn invalidate_collection_aliases(
    receiver: &SsaVariable,
    facts: &DefinitionFacts,
    invalidated_content_roots: &mut BTreeSet<SsaVariable>,
    invalidated_roots: &mut BTreeSet<SsaVariable>,
) {
    let Some(root) = collection_fact_root(receiver, facts, &mut BTreeSet::new()) else {
        return;
    };
    invalidated_content_roots.insert(root.clone());
    invalidated_roots.insert(root);
}

pub(super) fn apply_argument_field_writes(
    contract: &super::super::context::CallContract,
    argument_roots: &[Option<SsaVariable>],
    state: &mut BuildPassState<'_>,
) {
    for (index, root) in argument_roots.iter().enumerate() {
        if contract
            .argument_effects
            .get(index)
            .copied()
            .unwrap_or_default()
            != CollectionArgumentEffect::PreservesShape
        {
            continue;
        }
        let (Some(root), Some(writes)) = (root, contract.argument_field_writes.get(index)) else {
            continue;
        };
        state
            .indexed_collection_shapes
            .entry(root.clone())
            .or_default()
            .extend(writes.iter().map(|(field, shape)| (*field, *shape)));
    }
}

pub(super) fn update_indexed_shape_for_setitem(
    operands: &[SsaVariable],
    state: &mut BuildPassState<'_>,
) {
    let (Some(receiver), Some(index), Some(value)) =
        (operands.first(), operands.get(1), operands.get(2))
    else {
        return;
    };
    let Some(root) = collection_fact_root(receiver, state.definition_facts, &mut BTreeSet::new())
    else {
        return;
    };
    let selected_index =
        resolve_nonnegative_literal(index, state.definition_facts, &mut BTreeSet::new());
    let value_shape = collection_shape_facts_for_variable_from_state(value, state).shape;
    let mut indexed = state
        .indexed_collection_shapes
        .get(&root)
        .cloned()
        .or_else(|| {
            state
                .definition_facts
                .get(&root)
                .map(|fact| fact.indexed_shapes.clone())
        })
        .unwrap_or_default();
    match (selected_index, value_shape) {
        (Some(index), Some(shape)) => {
            indexed.insert(index, shape);
        }
        (Some(index), None) => {
            indexed.remove(&index);
        }
        (None, _) => indexed.clear(),
    }
    state.indexed_collection_shapes.insert(root, indexed);
}

pub(super) fn clear_indexed_collection_shapes(
    receiver: &SsaVariable,
    state: &mut BuildPassState<'_>,
) {
    let Some(root) = collection_fact_root(receiver, state.definition_facts, &mut BTreeSet::new())
    else {
        return;
    };
    state
        .indexed_collection_shapes
        .insert(root, BTreeMap::new());
}

pub(super) fn invalidate_collection_contents(
    receiver: &SsaVariable,
    facts: &DefinitionFacts,
    invalidated_content_roots: &mut BTreeSet<SsaVariable>,
) -> Option<SsaVariable> {
    let root = collection_fact_root(receiver, facts, &mut BTreeSet::new())?;
    invalidated_content_roots.insert(root.clone());
    Some(root)
}

pub(super) fn invalidate_all_collection_facts(
    facts: &DefinitionFacts,
    invalidated_content_roots: &mut BTreeSet<SsaVariable>,
    invalidated_roots: &mut BTreeSet<SsaVariable>,
) {
    invalidate_all_collection_facts_except(
        facts,
        invalidated_content_roots,
        invalidated_roots,
        &BTreeSet::new(),
    );
}

pub(super) fn invalidate_all_collection_facts_except(
    facts: &DefinitionFacts,
    invalidated_content_roots: &mut BTreeSet<SsaVariable>,
    invalidated_roots: &mut BTreeSet<SsaVariable>,
    shape_preserving_roots: &BTreeSet<SsaVariable>,
) {
    let roots = facts
        .keys()
        .filter_map(|variable| collection_fact_root(variable, facts, &mut BTreeSet::new()))
        .collect::<BTreeSet<_>>();
    invalidated_content_roots.extend(roots.iter().cloned());
    invalidated_roots.extend(
        roots
            .into_iter()
            .filter(|root| !shape_preserving_roots.contains(root)),
    );
}

pub(super) fn resolve_nonnegative_literal(
    variable: &SsaVariable,
    facts: &DefinitionFacts,
    visited: &mut BTreeSet<SsaVariable>,
) -> Option<usize> {
    if !visited.insert(variable.clone()) {
        return None;
    }
    match &facts.get(variable)?.expression {
        SsaExpr::Literal(Literal::Int(value)) => usize::try_from(*value).ok(),
        SsaExpr::Literal(Literal::BigInt(value)) => value.parse().ok(),
        SsaExpr::Variable(source) => resolve_nonnegative_literal(source, facts, visited),
        _ => None,
    }
}

pub(super) fn resolves_to_null(
    variable: &SsaVariable,
    facts: &DefinitionFacts,
    visited: &mut BTreeSet<SsaVariable>,
) -> bool {
    if !visited.insert(variable.clone()) {
        return false;
    }
    match facts.get(variable).map(|fact| &fact.expression) {
        Some(SsaExpr::Literal(Literal::Null)) => true,
        Some(SsaExpr::Variable(source)) => resolves_to_null(source, facts, visited),
        _ => false,
    }
}

pub(super) fn resolve_nonnegative_i32_literal(
    variable: &SsaVariable,
    facts: &DefinitionFacts,
    visited: &mut BTreeSet<SsaVariable>,
) -> Option<usize> {
    if !facts.get(variable)?.is_integer_literal {
        return None;
    }
    let value = resolve_nonnegative_literal(variable, facts, visited)?;
    let max = usize::try_from(i32::MAX).ok()?;
    (value <= max).then_some(value)
}
