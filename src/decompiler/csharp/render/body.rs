use std::collections::BTreeMap;
use std::fmt::Write;

use crate::instruction::Instruction;

use super::super::super::high_level::HighLevelEmitter;
use super::super::helpers::{
    csharpize_statement, csharpize_statement_typed, line_is_csharp_terminator, SlotTypes,
};

pub(super) struct LiftedBodyContext<'a> {
    pub(super) method_labels_by_offset: &'a BTreeMap<usize, String>,
    pub(super) method_arg_counts_by_offset: &'a BTreeMap<usize, usize>,
    pub(super) method_returns_value_by_offset: &'a BTreeMap<usize, bool>,
    pub(super) call_targets_by_offset: &'a BTreeMap<usize, usize>,
    pub(super) calla_targets_by_offset: &'a BTreeMap<usize, usize>,
    pub(super) callt_labels: &'a [String],
    pub(super) callt_param_counts: &'a [usize],
    pub(super) callt_returns_value: &'a [bool],
    pub(super) inline_single_use_temps: bool,
    pub(super) emit_trace_comments: bool,
    /// When true, body-local declarations use inferred C# types (`BigInteger
    /// loc0 = ...;`) instead of `var`. Off by default to preserve historical
    /// output.
    pub(super) typed_declarations: bool,
    /// Inferred C# slot types keyed by method start offset. Only consulted when
    /// `typed_declarations` is true.
    pub(super) slot_types_by_offset: &'a BTreeMap<usize, SlotTypes>,
}

pub(super) fn write_lifted_body(
    output: &mut String,
    instructions: &[Instruction],
    argument_labels: Option<&[String]>,
    warnings: &mut Vec<String>,
    context: &LiftedBodyContext<'_>,
    returns_void: bool,
) {
    if instructions.len() > super::super::super::high_level::MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS {
        let offset = instructions.first().map(|i| i.offset).unwrap_or(0);
        writeln!(
            output,
            "            // method body too large for high-level lifting: {} instructions \
             exceeds the {}-instruction limit; use `disasm` for the full listing",
            instructions.len(),
            super::super::super::high_level::MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS
        )
        .unwrap();
        warnings.push(format!(
            "csharp: method at 0x{offset:04X} skipped — {} instructions exceeds the \
             high-level lifting limit ({})",
            instructions.len(),
            super::super::super::high_level::MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS
        ));
        return;
    }
    let mut emitter = HighLevelEmitter::with_program(instructions);
    if let Some(labels) = argument_labels {
        emitter.set_argument_labels(labels);
    }
    emitter.set_callt_labels(context.callt_labels.to_vec());
    emitter.set_callt_param_counts(context.callt_param_counts.to_vec());
    emitter.set_callt_returns_value(context.callt_returns_value.to_vec());
    emitter.set_method_labels_by_offset(context.method_labels_by_offset);
    emitter.set_method_arg_counts_by_offset(context.method_arg_counts_by_offset);
    emitter.set_method_returns_value_by_offset(context.method_returns_value_by_offset);
    emitter.set_call_targets_by_offset(context.call_targets_by_offset);
    emitter.set_calla_targets_by_offset(context.calla_targets_by_offset);
    emitter.set_returns_void(returns_void);
    emitter.set_inline_single_use_temps(context.inline_single_use_temps);
    emitter.set_emit_trace_comments(context.emit_trace_comments);
    for instruction in instructions {
        emitter.advance_to(instruction.offset);
        emitter.emit_instruction(instruction);
    }
    let result = emitter.finish();
    warnings.extend(result.warnings);
    let mut statements = result.statements;
    if statements.is_empty() {
        // A non-void method with no body fails to compile (C# error CS0161:
        // not all code paths return a value). Mirror the empty-slice stub in
        // render/methods.rs and throw; void methods can keep the bare comment
        // since C# supplies the implicit return.
        if returns_void {
            writeln!(output, "            // no instructions decoded").unwrap();
        } else {
            writeln!(output, "            throw new NotImplementedException();").unwrap();
        }
        return;
    }

    // C# void methods receive an implicit return at the end of the body, so
    // the explicit trailing `return;` lifted from the bytecode RET is
    // redundant clutter. Drop it when it is the final non-blank statement.
    if returns_void {
        if let Some(last_idx) = statements.iter().rposition(|s| !s.trim().is_empty()) {
            if statements[last_idx].trim() == "return;" {
                statements[last_idx].clear();
            }
        }
    }

    let method_start = instructions.first().map(|i| i.offset).unwrap_or(0);
    let slot_types = if context.typed_declarations {
        context
            .slot_types_by_offset
            .get(&method_start)
            .cloned()
            .unwrap_or_default()
    } else {
        SlotTypes::default()
    };

    // Track which open braces correspond to a `case`/`default` body so we
    // can synthesise a trailing `break;` before the matching close. C#
    // forbids implicit fall-through, so a case body that does not already
    // end in a control-transfer statement (return/throw/goto/break) needs
    // the explicit `break;` to compile.
    let mut indent_level = 0usize;
    let mut block_kinds: Vec<BlockKind> = Vec::new();
    let mut last_emitted: Option<String> = None;
    for line in statements {
        let converted = if context.typed_declarations {
            csharpize_statement_typed(&line, &slot_types)
        } else {
            csharpize_statement(&line)
        };
        let trimmed = converted.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed == "}" {
            indent_level = indent_level.saturating_sub(1);
            if matches!(block_kinds.last(), Some(BlockKind::Case)) {
                let needs_break = last_emitted
                    .as_deref()
                    .map(|prev| !line_is_csharp_terminator(prev))
                    .unwrap_or(true);
                if needs_break {
                    let break_indent = 12 + (indent_level + 1) * 4;
                    writeln!(output, "{:indent$}break;", "", indent = break_indent).unwrap();
                }
            }
            block_kinds.pop();
        } else if trimmed.starts_with('}') {
            // `} else { ... } else if (...) { ...`: closes one block and
            // opens another. Pop the closed block's kind; the new opener
            // is pushed below if the line ends with `{`.
            indent_level = indent_level.saturating_sub(1);
            block_kinds.pop();
        }

        let rendered = if !returns_void && trimmed == "return;" {
            "return default;"
        } else {
            trimmed
        };

        let indent = 12 + indent_level * 4;
        writeln!(output, "{:indent$}{}", "", rendered, indent = indent).unwrap();

        if rendered.ends_with('{') {
            let kind = if rendered.starts_with("case ") || rendered.starts_with("default:") {
                BlockKind::Case
            } else {
                BlockKind::Other
            };
            block_kinds.push(kind);
            indent_level += 1;
        }

        last_emitted = Some(rendered.to_string());
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    Case,
    Other,
}
