//! Lower [`SsaForm`] back to the typed [`crate::decompiler::ir`] for rendering.
//!
//! This is the bridge that makes the Phase 2/3 SSA work (def/use chains, φ
//! placement, constant folding / copy propagation / DCE) visible as readable
//! pseudo-code. Each SSA block is rendered as a labelled section: φ nodes first
//! (as `x = φ(...)` lines), then the surviving assignments. Folded constants
//! and propagated copies appear inline, and dead defs are already gone.
//!
//! The lowering is the structural inverse of [`super::convert`] (`IR → SSA`).

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::decompiler::ir::Expr;

use super::form::{SsaExpr, SsaForm, SsaStmt};
use super::variable::{PhiNode, SsaVariable};

/// Lower an SSA expression to the typed IR expression tree.
#[must_use]
pub fn ssa_expr_to_ir(expr: &SsaExpr) -> Expr {
    ssa_expr_to_ir_with_source_names(expr, &BTreeMap::new())
}

pub(crate) fn ssa_expr_to_ir_with_source_names(
    expr: &SsaExpr,
    source_names: &BTreeMap<String, String>,
) -> Expr {
    match expr {
        SsaExpr::Literal(lit) => Expr::Literal(lit.clone()),
        SsaExpr::Variable(var) => Expr::Variable(ssa_var_name(var, source_names)),
        SsaExpr::Binary { op, left, right } => Expr::binary(
            *op,
            ssa_expr_to_ir_with_source_names(left, source_names),
            ssa_expr_to_ir_with_source_names(right, source_names),
        ),
        SsaExpr::Unary { op, operand } => {
            Expr::unary(*op, ssa_expr_to_ir_with_source_names(operand, source_names))
        }
        SsaExpr::Call { name, args } => Expr::call(
            name.clone(),
            args.iter()
                .map(|arg| ssa_expr_to_ir_with_source_names(arg, source_names))
                .collect(),
        ),
        SsaExpr::Index { base, index } => Expr::index(
            ssa_expr_to_ir_with_source_names(base, source_names),
            ssa_expr_to_ir_with_source_names(index, source_names),
        ),
        SsaExpr::Member { base, name } => Expr::Member {
            base: Box::new(ssa_expr_to_ir_with_source_names(base, source_names)),
            name: name.clone(),
        },
        SsaExpr::Cast { expr, target_type } => Expr::Cast {
            expr: Box::new(ssa_expr_to_ir_with_source_names(expr, source_names)),
            target_type: target_type.clone(),
        },
        SsaExpr::Array(els) => Expr::Array(
            els.iter()
                .map(|expr| ssa_expr_to_ir_with_source_names(expr, source_names))
                .collect(),
        ),
        SsaExpr::Map(pairs) => Expr::Map(
            pairs
                .iter()
                .map(|(key, value)| {
                    (
                        ssa_expr_to_ir_with_source_names(key, source_names),
                        ssa_expr_to_ir_with_source_names(value, source_names),
                    )
                })
                .collect(),
        ),
        SsaExpr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => Expr::Ternary {
            condition: Box::new(ssa_expr_to_ir_with_source_names(condition, source_names)),
            then_expr: Box::new(ssa_expr_to_ir_with_source_names(then_expr, source_names)),
            else_expr: Box::new(ssa_expr_to_ir_with_source_names(else_expr, source_names)),
        },
    }
}

/// Render a whole [`SsaForm`] as readable pseudo-code text.
///
/// Each block is introduced by a `// block <id>` header; φ nodes render as
/// `target = φ(pred: value, …)`; assignments render via the IR expression
/// renderer so folded constants / propagated copies appear inline.
#[must_use]
pub fn render_ssa_form(ssa: &SsaForm) -> String {
    let mut out = String::new();
    let stats = ssa.stats();
    let _ = writeln!(
        out,
        "// Optimized SSA — {} blocks, {} φ nodes, {} statements, {} variables",
        stats.block_count, stats.total_phi_nodes, stats.total_statements, stats.total_variables
    );

    for (id, block) in ssa.blocks_iter() {
        let _ = writeln!(out, "// block {:?}", id);
        for phi in &block.phi_nodes {
            let _ = writeln!(out, "    {};", render_phi(phi));
        }
        if block.stmts.is_empty() && block.phi_nodes.is_empty() {
            let _ = writeln!(out, "    // (empty)");
        }
        for stmt in &block.stmts {
            let _ = writeln!(out, "    {};", render_ssa_stmt(stmt));
        }
    }

    out
}

/// Render a single SSA statement as text without the trailing semicolon.
fn render_ssa_stmt(stmt: &SsaStmt) -> String {
    match stmt {
        SsaStmt::Assign { target, value } => {
            format!(
                "{} = {}",
                ssa_var_name(target, &BTreeMap::new()),
                render_ir_expr(value)
            )
        }
        SsaStmt::Expr(value) => render_ir_expr(value),
        SsaStmt::Return(Some(value)) => format!("return {}", render_ir_expr(value)),
        SsaStmt::Return(None) => "return".to_string(),
        SsaStmt::Phi(phi) => render_phi(phi),
        SsaStmt::Other(inner) => match inner {
            crate::decompiler::ir::Stmt::Comment(text) => format!("// {text}"),
            other => format!("// {:?}", other),
        },
    }
}

/// Render an SSA expression via the IR renderer (after lowering).
fn render_ir_expr(expr: &SsaExpr) -> String {
    crate::decompiler::ir::render_expr(&ssa_expr_to_ir(expr))
}

/// Render a φ node as `target = φ(pred: value, …)`.
fn render_phi(phi: &PhiNode) -> String {
    let mut parts: Vec<String> = phi
        .operands
        .iter()
        .map(|(pred, var)| format!("{}: {}", pred.0, ssa_var_name(var, &BTreeMap::new())))
        .collect();
    parts.sort();
    format!(
        "{} = φ({})",
        ssa_var_name(&phi.target, &BTreeMap::new()),
        parts.join(", ")
    )
}

/// Human-readable name for an SSA variable: the base name plus version (so the
/// single-assignment property is visible — IR rendering is analysis-facing).
pub(crate) fn ssa_var_name(var: &SsaVariable, source_names: &BTreeMap<String, String>) -> String {
    if is_unknown(var) {
        "?".to_string()
    } else if let Some(source_name) = source_names.get(&var.base) {
        source_name.clone()
    } else {
        let generated = format!("{}_{}", var.base, var.version);
        source_names.get(&generated).cloned().unwrap_or(generated)
    }
}

fn is_unknown(v: &SsaVariable) -> bool {
    v.base == "?"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompiler::cfg::ssa::{SsaBlock, SsaExpr, SsaStmt, SsaVariable};
    use crate::decompiler::cfg::BlockId;
    use crate::decompiler::ir::{BinOp, Literal};

    fn v(base: &str, ver: usize) -> SsaVariable {
        SsaVariable::new(base.to_string(), ver)
    }

    #[test]
    fn lowers_binary_and_literal_to_ir() {
        let expr = SsaExpr::binary(
            BinOp::Add,
            SsaExpr::lit(Literal::Int(1)),
            SsaExpr::lit(Literal::Int(2)),
        );
        let ir = ssa_expr_to_ir(&expr);
        let rendered = crate::decompiler::ir::render_expr(&ir);
        assert_eq!(rendered, "(1 + 2)");
    }

    #[test]
    fn render_form_shows_block_header_and_assignments() {
        let mut block = SsaBlock::new();
        block.add_stmt(SsaStmt::assign(v("b0", 0), SsaExpr::lit(Literal::Int(7))));
        block.add_stmt(SsaStmt::assign(v("b0", 1), SsaExpr::var(v("b0", 0))));
        let mut blocks = std::collections::BTreeMap::new();
        blocks.insert(BlockId(0), block);

        let ssa = SsaForm {
            cfg: crate::decompiler::cfg::Cfg::new(),
            dominance: super::super::dominance::DominanceInfo::new(),
            blocks,
            definitions: std::collections::BTreeMap::new(),
            uses: std::collections::BTreeMap::new(),
        };

        let text = render_ssa_form(&ssa);
        assert!(
            text.contains("// block"),
            "should render a block header: {text}"
        );
        assert!(
            text.contains("b0_0 = 7"),
            "should render the literal assign: {text}"
        );
    }

    #[test]
    fn render_phi_lists_one_operand_per_predecessor() {
        use crate::decompiler::cfg::ssa::PhiNode;
        let mut phi = PhiNode::new(v("p3", 0));
        phi.add_operand(crate::decompiler::cfg::BlockId(1), v("b1", 0));
        phi.add_operand(crate::decompiler::cfg::BlockId(2), v("b2", 0));
        let line = render_phi(&phi);
        assert!(line.starts_with("p3_0 = φ("));
        assert!(line.contains("1: b1_0"));
        assert!(line.contains("2: b2_0"));
    }
}
