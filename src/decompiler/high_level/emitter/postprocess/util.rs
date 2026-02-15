//! Shared helper routines for postprocessing lifted statements.

mod analysis;
mod blocks;
mod ident;
mod model;
mod parsing;
mod patterns;
mod scan;

pub(super) use model::Assignment;
pub(super) use patterns::{
    extract_any_if_condition, extract_else_if_condition, extract_if_condition, is_else_if_open,
    is_else_open, is_if_open,
};
