use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::analysis::call_graph::{CallEdge, CallGraph, CallTarget};
use crate::decompiler::analysis::method_contracts::{
    MethodContract, MethodContracts, ReturnBehavior,
};
use crate::decompiler::analysis::types::{TypeInfo, ValueType};
use crate::decompiler::analysis::MethodRef;
use crate::decompiler::cfg::method_body::{LoweringIssueKind, SymbolInfo, SymbolOrigin};
use crate::decompiler::ir::{
    BinOp, Block, ControlFlow, Expr, Intrinsic, Literal, SemanticCallTarget, Stmt, UnaryOp,
};
use crate::decompiler::output_format::RenderOptions;
use crate::instruction::{Instruction, OpCode, Operand};
use crate::manifest::ContractManifest;
use crate::nef::{NefFile, NefHeader};

use super::expr::{render_expr, render_vm_condition, ExprContext};
use super::expr_syscalls::known_syscall_is_classified;
use super::plan::{
    build_csharp_method_plans, plan_contract_symbols, plan_declarations, DeclarationKind,
};
use super::stmt::{render_block, terminates};

fn method_contract(
    offset: usize,
    name: &str,
    argument_count: usize,
    return_behavior: ReturnBehavior,
) -> MethodContract {
    MethodContract {
        method: MethodRef {
            offset,
            name: name.to_string(),
        },
        argument_count,
        return_behavior,
        may_return: true,
        return_shape: None,
        return_collection_facts: None,
        argument_effects: vec![
            crate::decompiler::cfg::ssa::CollectionArgumentEffect::Unknown;
            argument_count
        ],
        argument_collection_facts: vec![Default::default(); argument_count],
        argument_field_writes: vec![BTreeMap::new(); argument_count],
    }
}

fn expr_context_with_types(types: &[(&str, ValueType)]) -> ExprContext {
    let symbols = types
        .iter()
        .map(|(name, value_type)| {
            (
                (*name).to_string(),
                SymbolInfo {
                    origin: SymbolOrigin::Local(0),
                    value_type: *value_type,
                },
            )
        })
        .collect();
    ExprContext::for_block(&Block::new(), &symbols, false)
}

#[path = "tests_expr.rs"]
mod expr;
#[path = "tests_expr_types.rs"]
mod expr_types;
#[path = "tests_plan.rs"]
mod plan;
#[path = "tests_stmt.rs"]
mod stmt;
