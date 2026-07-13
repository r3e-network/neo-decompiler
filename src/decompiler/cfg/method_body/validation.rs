use crate::decompiler::ir::{Block, ControlFlow, Expr, SemanticCallTarget, Stmt};
use crate::instruction::Instruction;

use super::{Fidelity, FidelityReport, LoweringIssue, LoweringIssueKind};

pub(super) fn validate_renderable(
    body: &Block,
    instructions: &[Instruction],
    fidelity: &mut FidelityReport,
) {
    let mut validation = Validation::default();
    validate_block(body, &mut validation);
    let Some(first) = instructions.first() else {
        return;
    };

    let mut add_issue = |kind, detail: &str| {
        if fidelity.issues.iter().any(|issue| issue.kind == kind) {
            return;
        }
        fidelity.issues.push(LoweringIssue {
            offset: first.offset,
            opcode: first.opcode,
            kind,
            fidelity: Fidelity::Incomplete,
            detail: detail.to_string(),
        });
    };
    if validation.unknown_value {
        add_issue(
            LoweringIssueKind::LostStackValue,
            "unknown value survives to structured output",
        );
    }
    if validation.unresolved_call {
        add_issue(
            LoweringIssueKind::UnresolvedCall,
            "unresolved call survives to structured output",
        );
    }
    if validation.missing_provenance {
        add_issue(
            LoweringIssueKind::MissingProvenance,
            "structured output contains a value without source provenance",
        );
    }
    if validation.unsupported_control {
        add_issue(
            LoweringIssueKind::UnsupportedControl,
            "structured output contains an unresolved control transfer",
        );
    }
}

#[derive(Default)]
struct Validation {
    unknown_value: bool,
    unresolved_call: bool,
    missing_provenance: bool,
    unsupported_control: bool,
}

fn validate_block(block: &Block, validation: &mut Validation) {
    for statement in &block.stmts {
        validate_statement(statement, validation);
    }
}

fn validate_statement(statement: &Stmt, validation: &mut Validation) {
    match statement {
        Stmt::Assign { value, .. } | Stmt::ExprStmt(value) => validate_expr(value, validation),
        Stmt::Return(value) | Stmt::Throw(value) | Stmt::Abort(value) => {
            if let Some(value) = value {
                validate_expr(value, validation);
            }
        }
        Stmt::Assert { condition, message } => {
            validate_expr(condition, validation);
            if let Some(message) = message {
                validate_expr(message, validation);
            }
        }
        Stmt::Comment(comment) => {
            validation.unsupported_control =
                comment.starts_with("return at ") || comment.starts_with("return/throw/abort at ");
        }
        Stmt::Break | Stmt::Continue | Stmt::Label(_) | Stmt::Goto(_) => {}
        Stmt::ControlFlow(control) => validate_control(control, validation),
    }
}

fn validate_control(control: &ControlFlow, validation: &mut Validation) {
    match control {
        ControlFlow::If {
            condition,
            then_branch,
            else_branch,
        } => {
            validate_expr(condition, validation);
            validate_block(then_branch, validation);
            if let Some(branch) = else_branch {
                validate_block(branch, validation);
            }
        }
        ControlFlow::While { condition, body } => {
            validate_expr(condition, validation);
            validate_block(body, validation);
        }
        ControlFlow::DoWhile { body, condition } => {
            validate_block(body, validation);
            validate_expr(condition, validation);
        }
        ControlFlow::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                validate_statement(init, validation);
            }
            if let Some(condition) = condition {
                validate_expr(condition, validation);
            }
            if let Some(update) = update {
                validate_expr(update, validation);
            }
            validate_block(body, validation);
        }
        ControlFlow::TryCatch {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            validate_block(try_body, validation);
            if let Some(body) = catch_body {
                validate_block(body, validation);
            }
            if let Some(body) = finally_body {
                validate_block(body, validation);
            }
        }
        ControlFlow::Switch {
            expr,
            cases,
            default,
        } => {
            validate_expr(expr, validation);
            for (value, body) in cases {
                validate_expr(value, validation);
                validate_block(body, validation);
            }
            if let Some(body) = default {
                validate_block(body, validation);
            }
        }
    }
}

fn validate_expr(expression: &Expr, validation: &mut Validation) {
    match expression {
        Expr::Variable(name) => validation.unknown_value |= name == "?",
        Expr::Binary { left, right, .. } => {
            validate_expr(left, validation);
            validate_expr(right, validation);
        }
        Expr::Unary { operand, .. } => validate_expr(operand, validation),
        Expr::Call { target, args } => {
            validation.unresolved_call |= matches!(target, SemanticCallTarget::Unresolved { .. });
            for argument in args {
                validate_expr(argument, validation);
            }
        }
        Expr::Index { base, index } => {
            validate_expr(base, validation);
            validate_expr(index, validation);
        }
        Expr::Member { base, .. } | Expr::Cast { expr: base, .. } => {
            validate_expr(base, validation)
        }
        Expr::Convert { value, .. } | Expr::IsType { value, .. } => {
            validate_expr(value, validation)
        }
        Expr::NewArray { length, .. } => validate_expr(length, validation),
        Expr::Array(values) | Expr::Struct(values) => {
            for value in values {
                validate_expr(value, validation);
            }
        }
        Expr::Map(pairs) => {
            for (key, value) in pairs {
                validate_expr(key, validation);
                validate_expr(value, validation);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            validate_expr(condition, validation);
            validate_expr(then_expr, validation);
            validate_expr(else_expr, validation);
        }
        Expr::StackTemp(_) => validation.missing_provenance = true,
        Expr::Unknown => validation.unknown_value = true,
        Expr::Literal(_) => {}
    }
}
