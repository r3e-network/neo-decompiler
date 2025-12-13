//! Control flow IR nodes for decompiled code.

use super::expression::Expr;
use super::statement::Block;

/// Control flow constructs in the IR.
#[derive(Debug, Clone, PartialEq)]
pub enum ControlFlow {
    /// If-else statement.
    If {
        condition: Expr,
        then_branch: Block,
        else_branch: Option<Block>,
    },
    /// While loop.
    While { condition: Expr, body: Block },
    /// Do-while loop.
    DoWhile { body: Block, condition: Expr },
    /// For loop.
    For {
        init: Option<Box<super::statement::Stmt>>,
        condition: Option<Expr>,
        update: Option<Expr>,
        body: Block,
    },
    /// Try-catch-finally block.
    TryCatch {
        try_body: Block,
        catch_var: Option<String>,
        catch_body: Option<Block>,
        finally_body: Option<Block>,
    },
    /// Switch statement (for Neo's SWITCH opcode).
    Switch {
        expr: Expr,
        cases: Vec<(Expr, Block)>,
        default: Option<Block>,
    },
}

impl ControlFlow {
    /// Create an if statement without else branch.
    pub fn if_then(condition: Expr, then_branch: Block) -> Self {
        ControlFlow::If {
            condition,
            then_branch,
            else_branch: None,
        }
    }

    /// Create an if-else statement.
    pub fn if_else(condition: Expr, then_branch: Block, else_branch: Block) -> Self {
        ControlFlow::If {
            condition,
            then_branch,
            else_branch: Some(else_branch),
        }
    }

    /// Create a while loop.
    pub fn while_loop(condition: Expr, body: Block) -> Self {
        ControlFlow::While { condition, body }
    }

    /// Create a do-while loop.
    pub fn do_while(body: Block, condition: Expr) -> Self {
        ControlFlow::DoWhile { body, condition }
    }

    /// Create a for loop.
    pub fn for_loop(
        init: Option<super::statement::Stmt>,
        condition: Option<Expr>,
        update: Option<Expr>,
        body: Block,
    ) -> Self {
        ControlFlow::For {
            init: init.map(Box::new),
            condition,
            update,
            body,
        }
    }

    /// Create a try-catch block.
    pub fn try_catch(
        try_body: Block,
        catch_var: Option<String>,
        catch_body: Option<Block>,
        finally_body: Option<Block>,
    ) -> Self {
        ControlFlow::TryCatch {
            try_body,
            catch_var,
            catch_body,
            finally_body,
        }
    }
}
