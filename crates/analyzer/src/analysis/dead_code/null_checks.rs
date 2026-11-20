//! `isset($id)` is redundant once $id is known to be defined and non-null.
//! A maybe-defined variable still needs the check.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NullCheck {
    Isset,
    Coalesce,
}

impl Analyzer<'_> {
    pub fn constant_null_check(
        &mut self,
        expression: &Expression,
        scope: &Scope,
        check: NullCheck,
    ) {
        if self.level < 4 {
            return;
        }
        let (identifier, label) = match check {
            NullCheck::Isset => ("isset", "in isset()"),
            NullCheck::Coalesce => ("nullCoalesce", "on left side of ??"),
        };
        if let ExpressionKind::Variable(Variable::Named(name)) = &expression.kind {
            let variable = scope.get(name);
            if scope.dynamic
                || !scope.variables.contains_key(name) && variable.definition != Definition::Absent
            {
                return;
            }
            let reason = match (variable.definition, variable.ty) {
                (Definition::Absent, _) => "is never defined",
                (Definition::Definite, ValueType::Null) => "always exists and is always null",
                (Definition::Definite, ty) if non_nullable(&ty) => {
                    "always exists and is not nullable"
                }
                _ => return,
            };
            let provenance = self.narrowed_at(expression, scope);
            self.diagnose_dead_code(
                &format!("{identifier}.variable"),
                expression.span,
                format!("Variable ${name} {label} {reason}.{provenance}"),
            );
        } else if let ExpressionKind::Index {
            target,
            index: Some(index),
        } = &expression.kind
        {
            self.check_absent_null_check_offset(
                expression, target, index, scope, identifier, label,
            );
        } else if check == NullCheck::Coalesce
            && let ExpressionKind::Literal(literal) = &expression.kind
        {
            let reason = if matches!(literal, Literal::Null) {
                "is always null"
            } else {
                "is not nullable"
            };
            self.diagnose_dead_code(
                "nullCoalesce.expr",
                expression.span,
                format!("Expression {label} {reason}."),
            );
        }
    }

    fn check_absent_null_check_offset(
        &mut self,
        expression: &Expression,
        target: &Expression,
        index: &Expression,
        scope: &Scope,
        identifier: &str,
        label: &str,
    ) {
        if scope.facts.has_expression(expression) {
            return;
        }
        let (Some(place), Some(key)) = (
            crate::scope::Place::of(target),
            crate::constant_types::literal(index),
        ) else {
            return;
        };
        let ty = self.place_type(&place, scope);
        let ValueType::Shape(shape) = &ty else {
            return;
        };
        if shape.kind == crate::types::ShapeKind::Object || shape.remainder.is_some() {
            return;
        }
        if super::super::arrays::shape_key(&key)
            .is_some_and(|key| shape.fields.iter().all(|field| field.key != key))
        {
            self.diagnose_dead_code(
                &format!("{identifier}.offset"),
                expression.span,
                format!("Offset {key} on {ty} {label} does not exist."),
            );
        }
    }
}

fn non_nullable(ty: &ValueType) -> bool {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        match ty {
            ValueType::Union(types) | ValueType::BenevolentUnion(types) => {
                pending.extend(types.iter())
            }
            ValueType::Conditional(conditional) => {
                pending.extend([&conditional.yes, &conditional.no])
            }
            ValueType::Template(template) => pending.push(&template.bound),
            ValueType::TypeConstant(_)
            | ValueType::Projection(_)
            | ValueType::Intersection(_)
            | ValueType::Void
            | ValueType::Null
            | ValueType::Mixed
            | ValueType::ExplicitMixed
            | ValueType::ImplicitMixed => return false,
            _ => {}
        }
    }
    true
}
