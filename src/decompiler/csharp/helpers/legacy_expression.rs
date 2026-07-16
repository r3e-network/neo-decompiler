//! Compatibility rewrites for the legacy high-level C# test renderer.
//!
//! The production renderer uses structured IR. These helpers remain available
//! to regression tests that exercise the older line-oriented lift, but the
//! implementation is split by rewrite category so changes stay localized.

#[cfg(test)]
#[path = "legacy_expression/collections.rs"]
mod collections;
#[cfg(test)]
#[path = "legacy_expression/literals.rs"]
mod literals;
#[cfg(test)]
#[path = "legacy_expression/numeric.rs"]
mod numeric;
#[cfg(test)]
#[path = "legacy_expression_scanner.rs"]
mod scanner;

#[cfg(test)]
pub(super) fn is_decimal_integer_literal(text: &str) -> bool {
    collections::is_decimal_integer_literal(text)
}
#[cfg(test)]
pub(super) use scanner::split_top_level_comma;

/// Rewrite one lifted expression into a compilable C# expression.
#[cfg(test)]
pub(super) fn legacy_expression_to_csharp(text: &str) -> String {
    numeric::rewrite_numeric_helpers(&scanner::rewrite_cat_operator(text))
}
