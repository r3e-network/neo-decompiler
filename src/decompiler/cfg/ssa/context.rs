use std::collections::BTreeMap;

/// Stack contract for one resolved call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CallContract {
    pub(crate) name: String,
    pub(crate) argument_count: usize,
    pub(crate) returns_value: bool,
}

impl CallContract {
    #[must_use]
    pub(crate) fn new(name: impl Into<String>, argument_count: usize, returns_value: bool) -> Self {
        Self {
            name: name.into(),
            argument_count,
            returns_value,
        }
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
