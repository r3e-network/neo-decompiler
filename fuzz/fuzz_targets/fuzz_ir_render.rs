//! Fuzz the IR rendering layer directly with structured IR inputs.
//!
//! This target constructs typed `Block`/`Stmt`/`ControlFlow`/`Expr` values
//! from raw fuzzer bytes and calls both renderers, asserting invariants that
//! must always hold regardless of the input structure:
//!
//! 1. No panic.
//! 2. Brace balance: `{` count == `}` count.
//! 3. Analysis dialect (`render_block`) is also brace-balanced.
#![no_main]

use libfuzzer_sys::fuzz_target;
use neo_decompiler::decompiler::ir::{
    BinOp, Block, ControlFlow, Expr, Literal, Stmt, UnaryOp,
    render_block, render_high_level_block,
};

/// Wraps a fuzzer byte slice and reads decisions from it, cycling when exhausted.
struct R<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> R<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn byte(&mut self) -> u8 {
        if self.data.is_empty() {
            return 0;
        }
        let v = self.data[self.pos % self.data.len()];
        self.pos += 1;
        v
    }
}

// ---- IR constructors -------------------------------------------------------

fn build_expr(r: &mut R, depth: u8) -> Expr {
    // Cap recursion to avoid exponential blowup.
    let choices: u8 = if depth == 0 { 4 } else { 10 };
    match r.byte() % choices {
        0 => Expr::int(r.byte() as i64 - 128),
        1 => Expr::var(format!("v{}", r.byte() % 4)),
        2 => Expr::Literal(Literal::Bool(r.byte() & 1 == 0)),
        3 => Expr::Unknown,
        // Binary
        4 => {
            const OPS: [BinOp; 8] = [
                BinOp::Add, BinOp::Sub, BinOp::Mul, BinOp::Div,
                BinOp::Eq, BinOp::Ne, BinOp::Lt, BinOp::LogicalAnd,
            ];
            let op = OPS[(r.byte() as usize) % OPS.len()];
            Expr::Binary {
                op,
                left: Box::new(build_expr(r, depth - 1)),
                right: Box::new(build_expr(r, depth - 1)),
            }
        }
        // Unary
        5 => {
            const OPS: [UnaryOp; 4] = [UnaryOp::Neg, UnaryOp::Not, UnaryOp::Inc, UnaryOp::Dec];
            let op = OPS[(r.byte() as usize) % OPS.len()];
            Expr::Unary { op, operand: Box::new(build_expr(r, depth - 1)) }
        }
        // Ternary
        6 => Expr::Ternary {
            condition: Box::new(build_expr(r, depth - 1)),
            then_expr: Box::new(build_expr(r, depth - 1)),
            else_expr: Box::new(build_expr(r, depth - 1)),
        },
        // Array
        7 => {
            let len = (r.byte() % 4) as usize;
            Expr::Array((0..len).map(|_| build_expr(r, depth - 1)).collect())
        }
        // Member access
        8 => Expr::Member {
            base: Box::new(build_expr(r, depth - 1)),
            name: format!("f{}", r.byte() % 4),
        },
        // Index access
        _ => Expr::Index {
            base: Box::new(build_expr(r, depth - 1)),
            index: Box::new(build_expr(r, depth - 1)),
        },
    }
}

fn build_block(r: &mut R, depth: u8) -> Block {
    let count = (r.byte() % 4) as usize; // 0–3 statements
    let stmts: Vec<Stmt> = (0..count).map(|_| build_stmt(r, depth)).collect();
    Block::with_stmts(stmts)
}

fn build_stmt(r: &mut R, depth: u8) -> Stmt {
    let choices: u8 = if depth == 0 { 6 } else { 13 };
    match r.byte() % choices {
        0 => Stmt::assign(format!("v{}", r.byte() % 4), build_expr(r, 2)),
        1 => Stmt::Return(Some(build_expr(r, 2))),
        2 => Stmt::Return(None),
        3 => Stmt::Throw(Some(build_expr(r, 2))),
        4 => Stmt::ExprStmt(build_expr(r, 2)),
        5 => Stmt::Break,
        6 => Stmt::Continue,
        7 => Stmt::Comment("fuzz comment".into()),
        8 => Stmt::Abort(Some(build_expr(r, 2))),
        9 => Stmt::Assert {
            condition: build_expr(r, 2),
            message: None,
        },
        // ControlFlow variants
        10 => Stmt::ControlFlow(Box::new(build_if(r, depth - 1))),
        11 => Stmt::ControlFlow(Box::new(build_loop(r, depth - 1))),
        _ => Stmt::ControlFlow(Box::new(build_try_or_switch(r, depth - 1))),
    }
}

fn build_if(r: &mut R, depth: u8) -> ControlFlow {
    let has_else = r.byte() & 1 == 0;
    ControlFlow::If {
        condition: build_expr(r, 2),
        then_branch: build_block(r, depth),
        else_branch: has_else.then(|| build_block(r, depth)),
    }
}

fn build_loop(r: &mut R, depth: u8) -> ControlFlow {
    match r.byte() % 3 {
        0 => ControlFlow::While {
            condition: build_expr(r, 2),
            body: build_block(r, depth),
        },
        1 => ControlFlow::DoWhile {
            body: build_block(r, depth),
            condition: build_expr(r, 2),
        },
        _ => ControlFlow::for_loop(
            Some(Stmt::assign("i", Expr::int(0))),
            Some(Expr::Binary {
                op: BinOp::Lt,
                left: Box::new(Expr::var("i")),
                right: Box::new(Expr::int((r.byte() % 8) as i64)),
            }),
            Some(Expr::Unary { op: UnaryOp::Inc, operand: Box::new(Expr::var("i")) }),
            build_block(r, depth),
        ),
    }
}

fn build_try_or_switch(r: &mut R, depth: u8) -> ControlFlow {
    match r.byte() % 2 {
        0 => {
            // TryCatch — all combinations of catch/finally/var
            let has_catch = r.byte() & 1 == 0;
            let catch_var = (has_catch && r.byte() & 1 == 0).then_some("e".to_string());
            let has_finally = r.byte() & 1 == 0;
            ControlFlow::TryCatch {
                try_body: build_block(r, depth),
                catch_var,
                catch_body: has_catch.then(|| build_block(r, depth)),
                finally_body: has_finally.then(|| build_block(r, depth)),
            }
        }
        _ => {
            // Switch
            let case_count = (r.byte() % 5) as usize;
            let has_default = r.byte() & 1 == 0;
            let cases = (0..case_count)
                .map(|_| (build_expr(r, 1), build_block(r, depth)))
                .collect();
            ControlFlow::Switch {
                expr: build_expr(r, 1),
                cases,
                default: has_default.then(|| build_block(r, depth)),
            }
        }
    }
}

// ---- Invariant checks ------------------------------------------------------

fn brace_balanced(s: &str) -> bool {
    s.matches('{').count() == s.matches('}').count()
}

// ---- Fuzz target -----------------------------------------------------------

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    let mut r = R::new(data);
    let block = build_block(&mut r, 3);

    // High-level dialect
    let hl = render_high_level_block(&block, 0);
    assert!(brace_balanced(&hl), "high-level brace imbalance:\n{hl}");

    // Analysis dialect
    let analysis = render_block(&block, 0);
    assert!(brace_balanced(&analysis), "analysis brace imbalance:\n{analysis}");
});
