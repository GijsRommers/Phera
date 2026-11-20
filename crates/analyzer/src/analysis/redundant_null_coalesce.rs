//! With `array{email: ?string}`, `$row['email'] ?? null` adds nothing.
//! The key must exist on every path. An optional field still needs the fallback.

use super::*;
use crate::scope::{AccessStep, Place};
use crate::types::ShapeKind;

impl Analyzer<'_> {
    pub(super) fn redundant_null_coalesce(
        &mut self,
        left: &Expression,
        right: &Expression,
        scope: &Scope,
        operator: &str,
    ) {
        if self.level < 1
            || !self.options.unnecessary_null_coalesce
            || !matches!(right.kind, ExpressionKind::Literal(Literal::Null))
        {
            return;
        }
        let Some(ty) = self.always_present_type(left, scope) else {
            return;
        };
        if ty.excludes_null() || ty == ValueType::Null {
            return;
        }
        self.diagnose_dead_code(
            "nullCoalesce.unnecessary",
            left.span,
            format!(
                "Coalesce operator {operator} is unnecessary because the left side is always set \
                 and the right side is null."
            ),
        );
    }

    fn always_present_type(&self, expression: &Expression, scope: &Scope) -> Option<ValueType> {
        let place = Place::of(expression)?;
        let base = scope.get(&place.root);
        let mut ty = if place.root == "this" && self.context.object {
            ValueType::Class(self.context.this_class.clone()?)
        } else if base.definition == Definition::Definite {
            base.ty
        } else {
            return None;
        };
        for step in &place.members {
            let key = match step {
                AccessStep::IntegerOffset(key) => ValueType::IntLiteral(*key),
                AccessStep::StringOffset(key) => ValueType::StringLiteral(key.as_slice().into()),
                AccessStep::VariableOffset(key) => scope.get(key).ty,
                AccessStep::Property(name) => {
                    let property = self.index.property(ty.class_name()?, name)?;
                    if property.default == crate::index::PropertyDefault::Other {
                        return None;
                    }
                    ty = property.effective_type(self.index);
                    continue;
                }
                AccessStep::Method(..)
                | AccessStep::ClassName(..)
                | AccessStep::ArithmeticOffset(_) => return None,
            };
            let key = super::arrays::shape_key(&key)?;
            let ValueType::Shape(shape) = &ty else {
                return None;
            };
            if shape.kind == ShapeKind::Object {
                return None;
            }
            ty = shape
                .fields
                .iter()
                .find(|field| field.key == key && !field.optional)?
                .value
                .clone();
        }
        Some(ty)
    }
}
