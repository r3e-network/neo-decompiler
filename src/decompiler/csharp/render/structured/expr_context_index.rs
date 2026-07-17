//! Provenance-aware typing for constant indexes in structured C# expressions.

use std::collections::BTreeSet;

use crate::decompiler::ir::{Expr, Intrinsic, Literal, SemanticCallTarget};
use crate::instruction::OpCode;

use super::types;
use super::ExprContext;

impl ExprContext {
    /// Return whether the expression's rendered C# type is already the target
    /// type, rather than merely having a proven VM value type. An object-array
    /// index has a proven element only at runtime and therefore still needs a
    /// dynamic boundary cast at C# call and assignment sites.
    pub(in crate::decompiler::csharp::render::structured) fn is_statically_exact_csharp_type(
        &self,
        expression: &Expr,
        target_type: &str,
    ) -> bool {
        match expression {
            Expr::Call {
                target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Pickitem)),
                args,
            } => args
                .first()
                .and_then(|base| self.exact_csharp_type(base))
                .and_then(types::csharp_array_element_type)
                .is_some_and(|element_type| element_type == target_type),
            Expr::Index { base, .. } => self
                .exact_csharp_type(base)
                .and_then(types::csharp_array_element_type)
                .is_some_and(|element_type| element_type == target_type),
            _ => self.exact_csharp_type(expression) == Some(target_type),
        }
    }

    pub(super) fn exact_literal_index_type<'a>(
        &'a self,
        base: &'a Expr,
        index: &'a Expr,
    ) -> Option<&'a str> {
        let index = self.constant_index(index)?;
        let elements = self.array_elements(base)?;
        let element = elements.get(index)?;
        self.exact_csharp_type(element).or_else(|| {
            let type_name = types::csharp_type(self.value_type(element), true);
            (!type_name.eq_ignore_ascii_case("dynamic")).then_some(type_name)
        })
    }

    fn constant_index(&self, expression: &Expr) -> Option<usize> {
        fn resolve(
            context: &ExprContext,
            expression: &Expr,
            seen: &mut BTreeSet<String>,
        ) -> Option<usize> {
            match expression {
                Expr::Literal(Literal::Int(value)) => usize::try_from(*value).ok(),
                Expr::Literal(Literal::BigInt(value)) => value.parse().ok(),
                Expr::Variable(name) if seen.insert(name.clone()) => context
                    .inline_values
                    .get(name)
                    .and_then(|value| resolve(context, value, seen)),
                _ => None,
            }
        }

        resolve(self, expression, &mut BTreeSet::new())
    }
}
