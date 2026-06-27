//! Per-method view of a contract for the structured-IR renderer.

use crate::decompiler::analysis::{MethodRef, MethodTable};
use crate::decompiler::cfg::ssa::optimize_ssa;
use crate::decompiler::cfg::{structure_cfg, BasicBlock, BlockId, Cfg, EdgeKind, Terminator};
use crate::decompiler::helpers::format_manifest_type;
use crate::decompiler::ir::render_block;
use crate::instruction::Instruction;
use crate::manifest::ContractManifest;

/// A per-method view: the method's instruction slice and a self-contained
/// sub-CFG whose terminators do not leave the method (cross-range jumps are
/// rewritten to `Return`; the sub-CFG has a synthesised entry block).
#[allow(dead_code)] // `method` and `instructions` are used by render_method_body / render_envelope.
#[derive(Debug, Clone)]
pub(crate) struct MethodView {
    pub method: MethodRef,
    pub cfg: Cfg,
    pub instructions: Vec<Instruction>,
}

/// Build a sub-CFG for each method: select blocks whose first instruction
/// lies within the method's range, prepend a synthesised entry block, and
/// rewrite cross-range `Jump`/`Jmpif*`/`Jmpifnot*` terminators to `Return`
/// so the sub-CFG is self-contained. `instructions` is the whole-script
/// instruction stream; each `MethodView` receives the slice whose offsets
/// fall within the method's range so `SsaBuilder` only sees the method's
/// instructions.
#[allow(dead_code)] // wired up by Decompilation::render_structured_ir (Task 6).
pub(crate) fn extract_method_cfgs(
    whole: &Cfg,
    table: &MethodTable,
    instructions: &[Instruction],
) -> Vec<MethodView> {
    let mut out = Vec::new();
    for (start, end, method) in table.methods() {
        let method_instructions: Vec<_> = instructions
            .iter()
            .filter(|i| i.offset >= start && i.offset < end)
            .cloned()
            .collect();
        if let Some(view) = extract_one(whole, start, end, method.clone(), method_instructions) {
            out.push(view);
        }
    }
    out
}

#[allow(dead_code)] // used by extract_method_cfgs.
fn extract_one(
    whole: &Cfg,
    start: usize,
    end: usize,
    method: MethodRef,
    instructions: Vec<Instruction>,
) -> Option<MethodView> {
    let mut selected: Vec<&BasicBlock> = whole
        .blocks()
        .filter(|b| b.instruction_range.start < end && b.start_offset >= start)
        .collect();
    if selected.is_empty() {
        return None;
    }
    let entry_existing = selected.iter().find(|b| b.start_offset == start).copied();
    selected.sort_by_key(|b| b.id.0);
    // Pick a fresh block ID for the synthesised entry that does not collide
    // with any block in the whole CFG (the existing entry block, if any, has
    // id == start, so using a high unused ID avoids aliasing).
    let max_id = whole.blocks().map(|b| b.id.0).max().unwrap_or(0);
    let entry_id = BlockId(max_id + 1);
    let mut sub = Cfg::new();

    for b in &selected {
        let mut nb = (*b).clone();
        // Rewrite cross-range jumps on every selected block, including the
        // method entry: a cross-range Jump from the entry means the method
        // tail-calls / jumps out, which the decompiler renders as `return`.
        nb.terminator = rewrite_terminator(&nb.terminator, &selected);
        sub.add_block(nb);
    }

    let entry_terminator = match entry_existing {
        Some(e) if matches!(e.terminator, Terminator::Fallthrough { .. }) => {
            Terminator::Fallthrough { target: e.id }
        }
        Some(_) => Terminator::Jump {
            target: entry_existing.unwrap().id,
        },
        None => Terminator::Return,
    };
    sub.add_block(BasicBlock::new(
        entry_id,
        start,
        start,
        start..start,
        entry_terminator,
    ));
    if let Some(eid) = entry_existing.map(|e| e.id) {
        sub.add_edge(entry_id, eid, EdgeKind::Unconditional);
    }

    for b in &selected {
        for s in b.terminator.successors() {
            if sub.block(s).is_some() && s != b.id {
                sub.add_edge(b.id, s, EdgeKind::Unconditional);
            }
        }
    }

    Some(MethodView {
        method,
        cfg: sub,
        instructions,
    })
}

#[allow(dead_code)] // used by extract_one.
fn rewrite_terminator(term: &Terminator, selected: &[&BasicBlock]) -> Terminator {
    let in_range = |bid: BlockId| selected.iter().any(|b| b.id == bid);
    match term {
        Terminator::Jump { target } if !in_range(*target) => Terminator::Return,
        Terminator::Branch {
            then_target,
            else_target,
        } if !in_range(*then_target) || !in_range(*else_target) => Terminator::Return,
        _ => term.clone(),
    }
}

/// Render a method body as `fn name() -> ret { body }`. The `manifest`
/// provides the return type (looked up by method name); falls back to `void`
/// if the manifest is missing or the method is unknown.
#[allow(dead_code)] // wired up by render_envelope (Task 5).
pub(crate) fn render_method_body(view: &MethodView, manifest: Option<&ContractManifest>) -> String {
    let mut ssa =
        crate::decompiler::cfg::ssa::SsaBuilder::new(&view.cfg, &view.instructions).build();
    optimize_ssa(&mut ssa);
    let block = structure_cfg(&ssa);
    let body = render_block(&block, 0);
    let ret = method_return_type(view, manifest);
    let name = sanitize_name(&view.method.name);
    if body.trim().is_empty() {
        format!("    fn {name}() -> {ret} {{\n        // empty body\n    }}\n")
    } else {
        let indented = body
            .lines()
            .map(|l| {
                if l.is_empty() {
                    String::new()
                } else {
                    format!("        {l}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("    fn {name}() -> {ret} {{\n{indented}\n    }}\n")
    }
}

fn method_return_type(view: &MethodView, manifest: Option<&ContractManifest>) -> String {
    let Some(manifest) = manifest else {
        return "void".to_string();
    };
    manifest
        .abi
        .methods
        .iter()
        .find(|m| m.name == view.method.name)
        .map(|m| format_manifest_type(&m.return_type))
        .unwrap_or_else(|| "void".to_string())
}

fn sanitize_name(raw: &str) -> String {
    let s: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        "sub".to_string()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruction::{Instruction, OpCode};

    fn ins(offset: usize, op: OpCode) -> Instruction {
        Instruction::new(offset, op, None)
    }

    fn two_method_whole_cfg() -> (Cfg, Vec<Instruction>) {
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            2,
            0..2,
            Terminator::Jump {
                target: BlockId(10),
            },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(2),
            2,
            2,
            2..2,
            Terminator::Jump {
                target: BlockId(10),
            },
        ));
        cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::Unconditional);
        cfg.add_edge(BlockId(2), BlockId(10), EdgeKind::Unconditional);
        cfg.add_block(BasicBlock::new(
            BlockId(10),
            10,
            12,
            10..12,
            Terminator::Return,
        ));
        let instructions = vec![
            ins(0, OpCode::Push1),
            ins(1, OpCode::Ret),
            ins(10, OpCode::Push0),
            ins(11, OpCode::Ret),
        ];
        (cfg, instructions)
    }

    #[test]
    fn extract_produces_two_sub_cfgs_with_cross_range_jump_rewritten() {
        let (cfg, instructions) = two_method_whole_cfg();
        let manifest_json = r#"{"name":"C","abi":{"methods":[
            {"name":"main","parameters":[],"returntype":"Integer","offset":0},
            {"name":"helper","parameters":[],"returntype":"Integer","offset":10}
        ]}}"#;
        let manifest: crate::manifest::ContractManifest =
            serde_json::from_str(manifest_json).unwrap();
        let table = MethodTable::new(&instructions, Some(&manifest));
        let views = extract_method_cfgs(&cfg, &table, &instructions);
        assert_eq!(views.len(), 2, "expected two method spans");
        let a = &views[0];
        let a0 = a.cfg.block(BlockId(0)).expect("block 0 in A");
        assert!(matches!(a0.terminator, Terminator::Return));
        let b = &views[1];
        let b10 = b.cfg.block(BlockId(10)).expect("block 10 in B");
        assert!(matches!(b10.terminator, Terminator::Return));
        assert!(a.cfg.block(BlockId(0)).is_some());
    }

    #[test]
    fn render_method_body_emits_fn_with_return_type() {
        // A trivial method: PUSH1 ; RET → `fn main() -> Integer { ... return ... }`.
        let instructions = vec![
            Instruction::new(0, OpCode::Push1, None),
            Instruction::new(1, OpCode::Ret, None),
        ];
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(BlockId(0), 0, 2, 0..2, Terminator::Return));
        let view = MethodView {
            method: MethodRef {
                offset: 0,
                name: "main".to_string(),
            },
            cfg,
            instructions,
        };
        let manifest_json = r#"{"name":"C","abi":{"methods":[
            {"name":"main","parameters":[],"returntype":"Integer"}
        ]}}"#;
        let manifest: ContractManifest = serde_json::from_str(manifest_json).unwrap();
        let out = render_method_body(&view, Some(&manifest));
        assert!(out.contains("fn main() -> int"), "got:\n{out}");
        assert!(out.contains("return"), "got:\n{out}");
    }
}
