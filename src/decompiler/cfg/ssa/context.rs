use std::collections::BTreeMap;

use serde::Serialize;

use crate::decompiler::ir::SemanticCallTarget;

/// Exact fixed-length collection shape proven by SSA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "length")]
#[non_exhaustive]
pub enum CollectionShape {
    /// A fixed-length VM array.
    Array(usize),
    /// A fixed-length VM struct.
    Struct(usize),
}

impl CollectionShape {
    #[must_use]
    pub(crate) const fn len(self) -> usize {
        match self {
            Self::Array(length) | Self::Struct(length) => length,
        }
    }
}

/// Fixed collection facts retained without assuming element identity.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CollectionShapeFacts {
    /// Fixed outer collection shape, when proven.
    pub shape: Option<CollectionShape>,
    /// Fixed shapes of values stored at constant collection indexes.
    pub indexed: BTreeMap<usize, CollectionShape>,
}

impl CollectionShapeFacts {
    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.shape.is_none() && self.indexed.is_empty()
    }
}

/// Effect of one call argument on fixed collection shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CollectionArgumentEffect {
    /// The callee may change the argument's collection length.
    #[default]
    Unknown,
    /// The callee does not mutate or escape the argument.
    ReadOnly,
    /// The callee may change contents but preserves collection length.
    PreservesShape,
}

/// Stack contract for one resolved call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CallContract {
    pub(crate) target: SemanticCallTarget,
    pub(crate) argument_count: usize,
    pub(crate) returns_value: bool,
    pub(crate) may_return: bool,
    pub(crate) return_shape: Option<CollectionShape>,
    pub(crate) return_facts: Option<CollectionShapeFacts>,
    pub(crate) argument_effects: Vec<CollectionArgumentEffect>,
    pub(crate) argument_field_writes: Vec<BTreeMap<usize, CollectionShape>>,
}

impl CallContract {
    #[must_use]
    pub(crate) fn new(
        target: SemanticCallTarget,
        argument_count: usize,
        returns_value: bool,
    ) -> Self {
        Self {
            target,
            argument_count,
            returns_value,
            may_return: true,
            return_shape: None,
            return_facts: None,
            argument_effects: vec![CollectionArgumentEffect::Unknown; argument_count],
            argument_field_writes: vec![BTreeMap::new(); argument_count],
        }
    }

    #[must_use]
    pub(crate) fn with_may_return(mut self, may_return: bool) -> Self {
        self.may_return = may_return;
        self
    }

    #[must_use]
    pub(crate) fn with_return_shape(mut self, return_shape: Option<CollectionShape>) -> Self {
        self.return_shape = return_shape;
        self
    }

    #[must_use]
    pub(crate) fn with_return_facts(mut self, return_facts: Option<CollectionShapeFacts>) -> Self {
        if self.return_shape.is_none() {
            self.return_shape = return_facts.as_ref().and_then(|facts| facts.shape);
        }
        self.return_facts = return_facts;
        self
    }

    #[must_use]
    pub(crate) fn with_argument_effects(
        mut self,
        argument_effects: Vec<CollectionArgumentEffect>,
    ) -> Self {
        self.argument_effects = argument_effects;
        self
    }

    #[must_use]
    pub(crate) fn with_argument_field_writes(
        mut self,
        argument_field_writes: Vec<BTreeMap<usize, CollectionShape>>,
    ) -> Self {
        self.argument_field_writes = argument_field_writes;
        self
    }
}

/// Source and call metadata for one method SSA build.
#[derive(Debug, Clone, Default)]
pub(crate) struct MethodContext {
    pub(crate) argument_names: Vec<String>,
    pub(crate) arguments_on_entry_stack: bool,
    pub(crate) returns_value: Option<bool>,
    pub(crate) calls_by_offset: BTreeMap<usize, CallContract>,
    pub(crate) argument_collection_facts: Vec<CollectionShapeFacts>,
    pub(crate) static_collection_facts: BTreeMap<usize, CollectionShapeFacts>,
}

impl MethodContext {
    pub(crate) fn source_names(&self) -> BTreeMap<String, String> {
        self.argument_names
            .iter()
            .enumerate()
            .map(|(index, name)| (format!("arg{index}"), name.clone()))
            .collect()
    }
}
