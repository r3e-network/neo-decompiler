use std::collections::BTreeMap;

use crate::decompiler::ir::SemanticCallTarget;

/// Stack contract for one resolved call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CallContract {
    pub(crate) target: SemanticCallTarget,
    pub(crate) argument_count: usize,
    pub(crate) returns_value: bool,
    pub(crate) may_return: bool,
}

impl CallContract {
    #[must_use]
    pub(crate) const fn new(
        target: SemanticCallTarget,
        argument_count: usize,
        returns_value: bool,
    ) -> Self {
        Self {
            target,
            argument_count,
            returns_value,
            may_return: true,
        }
    }

    #[must_use]
    pub(crate) const fn with_may_return(mut self, may_return: bool) -> Self {
        self.may_return = may_return;
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
