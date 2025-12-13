//! Post-processing passes applied to lifted high-level statements.
//!
//! These passes are intentionally lightweight: they rewrite some common
//! Neo-compiler patterns (notably loops) into more idiomatic pseudo-code.

mod for_loops;
mod inline;
mod util;
