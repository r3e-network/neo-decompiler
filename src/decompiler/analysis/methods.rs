use std::collections::BTreeMap;

use serde::Serialize;

use crate::instruction::Instruction;
use crate::manifest::{ContractManifest, ManifestMethod};

use super::super::helpers::{find_manifest_entry_method, sanitize_identifier};

/// Reference to a (possibly inferred) method within a script.
///
/// When a manifest is present, `name` typically matches the ABI method name.
/// For internal helper routines without ABI metadata, `name` will be a
/// synthetic `sub_0x....` label.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct MethodRef {
    /// Method entry offset in bytecode.
    pub offset: usize,
    /// Human-readable method name.
    pub name: String,
}

impl MethodRef {
    pub(super) fn synthetic(offset: usize) -> Self {
        Self {
            offset,
            name: format!("sub_0x{offset:04X}"),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct MethodSpan {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) method: MethodRef,
}

/// Helper for mapping bytecode offsets to method ranges.
#[derive(Debug, Clone)]
pub struct MethodTable {
    spans: Vec<MethodSpan>,
    manifest_index_by_start: BTreeMap<usize, usize>,
}

impl MethodTable {
    /// Build a method table using the manifest ABI offsets when present.
    ///
    /// If the manifest does not cover the script entry region, a synthetic
    /// entry span is inserted to ensure that every offset resolves to some
    /// method.
    #[must_use]
    pub fn new(instructions: &[Instruction], manifest: Option<&ContractManifest>) -> Self {
        let script_start = instructions.first().map(|ins| ins.offset).unwrap_or(0);
        let script_end = instructions
            .last()
            .map(|ins| ins.offset.saturating_add(1))
            .unwrap_or(script_start);

        let mut spans = Vec::new();
        let mut manifest_index_by_start = BTreeMap::new();

        if let Some(manifest) = manifest {
            let mut methods: Vec<(usize, usize, &ManifestMethod)> = manifest
                .abi
                .methods
                .iter()
                .enumerate()
                .filter_map(|(idx, method)| method.offset.map(|off| (off as usize, idx, method)))
                .collect();
            methods.sort_by_key(|(off, _, _)| *off);

            for (pos, (start, idx, method)) in methods.iter().enumerate() {
                let end = methods
                    .get(pos + 1)
                    .map(|(next, _, _)| *next)
                    .unwrap_or(script_end);
                let name = sanitize_identifier(&method.name);
                spans.push(MethodSpan {
                    start: *start,
                    end,
                    method: MethodRef {
                        offset: *start,
                        name,
                    },
                });
                manifest_index_by_start.insert(*start, *idx);
            }

            let entry_name = find_manifest_entry_method(manifest, script_start)
                .map(|(method, _)| sanitize_identifier(&method.name))
                .unwrap_or_else(|| "script_entry".to_string());

            let needs_entry = spans
                .first()
                .map(|span| span.start > script_start)
                .unwrap_or(true);
            if needs_entry {
                let end = spans.first().map(|span| span.start).unwrap_or(script_end);
                spans.insert(
                    0,
                    MethodSpan {
                        start: script_start,
                        end,
                        method: MethodRef {
                            offset: script_start,
                            name: entry_name,
                        },
                    },
                );
            }
        } else {
            spans.push(MethodSpan {
                start: script_start,
                end: script_end,
                method: MethodRef {
                    offset: script_start,
                    name: "script_entry".to_string(),
                },
            });
        }

        spans.sort_by_key(|span| span.start);

        Self {
            spans,
            manifest_index_by_start,
        }
    }

    /// Return all known method spans ordered by start offset.
    pub(super) fn spans(&self) -> &[MethodSpan] {
        &self.spans
    }

    /// Resolve the method that contains the given bytecode offset.
    #[must_use]
    pub fn method_for_offset(&self, offset: usize) -> MethodRef {
        match self.spans.binary_search_by_key(&offset, |span| span.start) {
            Ok(index) => self.spans[index].method.clone(),
            Err(0) => self
                .spans
                .first()
                .map(|span| span.method.clone())
                .unwrap_or_else(|| MethodRef::synthetic(offset)),
            Err(index) => {
                let span = &self.spans[index - 1];
                span.method.clone()
            }
        }
    }

    /// Resolve an internal call target to a method reference.
    #[must_use]
    pub fn resolve_internal_target(&self, target_offset: usize) -> MethodRef {
        self.spans
            .iter()
            .find(|span| span.start == target_offset)
            .map(|span| span.method.clone())
            .unwrap_or_else(|| MethodRef::synthetic(target_offset))
    }

    /// Return the manifest ABI method index for a method starting at `offset`, if any.
    #[must_use]
    pub fn manifest_index_for_start(&self, offset: usize) -> Option<usize> {
        self.manifest_index_by_start.get(&offset).copied()
    }
}
