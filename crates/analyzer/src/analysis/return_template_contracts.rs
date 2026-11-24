//! `/** @return T */ function build(): Base { return new Base; }`
//! may return the base type when the caller's T is a subtype.

use super::*;
use crate::types::{CollectionKind, Template};

impl Analyzer<'_> {
    pub(super) fn mismatched_template_return(
        &self,
        expression: &Expression,
        expected: &ValueType,
        given: &ValueType,
    ) -> Option<String> {
        if self.level < 7
            || !matches!(
                expression.kind,
                ExpressionKind::New { .. }
                    | ExpressionKind::Call { .. }
                    | ExpressionKind::MethodCall { .. }
                    | ExpressionKind::StaticCall { .. }
            )
        {
            return None;
        }
        let (expected, nullable) = match expected {
            ValueType::Union(types) if types.len() == 2 && types.contains(&ValueType::Null) => {
                (types.iter().find(|ty| **ty != ValueType::Null)?, true)
            }
            expected => (expected, false),
        };
        let given = if nullable {
            given.without_null()
        } else {
            given.clone()
        };
        let description = match (expected, &given) {
            (ValueType::Template(template), ValueType::Class(_)) => template_description(template)?,
            (ValueType::Collection(expected), ValueType::Collection(given))
                if expected.kind == CollectionKind::Array
                    && matches!(given.value, ValueType::Class(_)) =>
            {
                let ValueType::Template(template) = &expected.value else {
                    return None;
                };
                format!("array<{}>", template_description(template)?)
            }
            _ => return None,
        };
        Some(if nullable {
            format!("({description})|null")
        } else {
            description
        })
    }
}

fn template_description(template: &Template) -> Option<String> {
    let bounds = match &template.bound {
        ValueType::Union(bounds) => bounds.as_ref(),
        bound => std::slice::from_ref(bound),
    };
    if !bounds
        .iter()
        .all(|bound| matches!(bound, ValueType::Class(_) | ValueType::Null))
        || !bounds
            .iter()
            .any(|bound| matches!(bound, ValueType::Class(_)))
    {
        return None;
    }
    let bound = super::member_checks::receiver_name(&template.bound);
    Some(format!("{} of {bound}", template.name))
}
