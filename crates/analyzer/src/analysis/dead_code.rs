//! A type check is constant when every value agrees. For example, an
//! `int` is never a string, but `int|string` can give either result.

use super::*;
use crate::types::{CollectionKind, ShapeKind, StringKind};

mod booleans;
mod comparisons;
mod discarded_results;
mod null_checks;
mod private_members;
mod type_tests;

pub use booleans::Side;
pub use null_checks::NullCheck;

impl Analyzer<'_> {
    pub fn trait_method_declaration(
        &mut self,
        method: &Method,
        member: &ClassMember,
        consumers: &[String],
    ) {
        if self.level < 4 {
            for consumer in consumers {
                self.context.class = Some(consumer.clone());
                self.method_declaration(method, member.span, member.doc_comment);
            }
            return;
        }
        let previous = self.trait_diagnostics.take();
        let mut common = None::<Vec<(Span, Diagnostic)>>;
        for consumer in consumers {
            self.context.class = Some(consumer.clone());
            self.trait_diagnostics = Some(Vec::new());
            self.method_declaration(method, member.span, member.doc_comment);
            let notices = self.trait_diagnostics.take().unwrap_or_default();
            if let Some(common) = &mut common {
                // A trait comparing $this with one of its consumers may give
                // different answers in different classes.
                common.retain(|(span, diagnostic)| {
                    notices.iter().any(|(other, other_diagnostic)| {
                        span == other && diagnostic.identifier == other_diagnostic.identifier
                    })
                });
            } else {
                common = Some(notices);
            }
        }
        self.trait_diagnostics = previous;
        for (span, diagnostic) in common.into_iter().flatten() {
            self.diagnose_dead_code(&diagnostic.identifier, span, diagnostic.message);
        }
    }

    pub fn diagnose_dead_code(&mut self, identifier: &str, span: Span, message: String) {
        if let Some(diagnostics) = &mut self.trait_diagnostics {
            diagnostics.push((
                span,
                Diagnostic {
                    file: self.file.path.clone(),
                    line: self.file.lines.line(span.start),
                    identifier: identifier.into(),
                    severity: crate::diagnostic::Severity::Error,
                    message,
                },
            ));
        } else {
            self.diagnose(identifier, span, message);
        }
    }

    pub fn check_nullsafe(
        &mut self,
        kind: &str,
        target: &Expression,
        member: &MemberName,
        ty: &ValueType,
        scope: &Scope,
        span: Span,
    ) {
        if self.level >= 4
            && ty.excludes_null()
            && matches!(
                ty,
                ValueType::Object
                    | ValueType::Class(_)
                    | ValueType::Generic(_)
                    | ValueType::Callable(_)
            )
        {
            let span = match member {
                MemberName::Identifier(name) => name.span,
                MemberName::Expression(_) => span,
            };
            let provenance = self.narrowed_at(target, scope);
            self.diagnose_dead_code(
                "nullsafe.neverNull",
                span,
                format!(
                    "Using nullsafe {kind} on non-nullable type {ty}. Use -> instead.{provenance}"
                ),
            );
        }
    }
}

impl Analyzer<'_> {
    pub fn discarded_statements(&mut self, statement: &Statement) {
        if self.level >= 4 {
            self.diagnose_dead_code(
                "deadCode.unreachable",
                statement.span,
                "Unreachable statement - code above always terminates.".into(),
            );
        }
    }

    pub fn constant_condition(
        &mut self,
        kind: &str,
        label: &str,
        expression: &Expression,
        outcome: &condition::Condition,
    ) {
        if self.level < 4 || condition_has_own_rule(expression) || self.type_test_call(expression) {
            return;
        }
        let Some(result) = outcome.truth() else {
            return;
        };
        if result && self.last_condition == Some(expression.span) {
            return;
        }
        self.constant_result(
            kind,
            expression.span,
            result,
            format!("{label} is always {result}."),
        );
    }

    fn constant_result(&mut self, kind: &str, span: Span, result: bool, message: String) {
        if result && self.last_condition == Some(span) {
            return;
        }
        let suffix = if result { "alwaysTrue" } else { "alwaysFalse" };
        self.diagnose_dead_code(&format!("{kind}.{suffix}"), span, message);
    }
}

pub fn condition_has_own_rule(expression: &Expression) -> bool {
    matches!(
        expression.kind,
        ExpressionKind::InstanceOf { .. }
            | ExpressionKind::Isset(_)
            | ExpressionKind::Empty(_)
            | ExpressionKind::Ternary { .. }
            | ExpressionKind::Unary {
                operator: UnaryOperator::Not,
                ..
            }
            | ExpressionKind::Binary {
                operator: BinaryOperator::And
                    | BinaryOperator::Or
                    | BinaryOperator::Equal
                    | BinaryOperator::NotEqual
                    | BinaryOperator::Identical
                    | BinaryOperator::NotIdentical
                    | BinaryOperator::Less
                    | BinaryOperator::LessEqual
                    | BinaryOperator::Greater
                    | BinaryOperator::GreaterEqual,
                ..
            }
    )
}

pub fn truth(ty: &ValueType) -> Option<bool> {
    use ValueType as T;
    let mut pending = vec![ty];
    let mut result = None;
    while let Some(ty) = pending.pop() {
        let value = match ty {
            T::Union(arms) | T::BenevolentUnion(arms) => {
                pending.extend(arms.iter());
                continue;
            }
            T::Never => continue,
            T::Null | T::Void => false,
            T::BoolLiteral(value) => *value,
            T::IntLiteral(value) => *value != 0,
            T::StringLiteral(bytes) => !bytes.is_empty() && &**bytes != b"0",
            T::IntRange(min, max) if *min > 0 || *max < 0 => true,
            T::IntRange(0, 0) => false,
            T::StringRefinement(StringKind::Truthy) | T::ClassString(_) => true,
            T::Object | T::Class(_) | T::Generic(_) | T::Callable(_) | T::This | T::Resource => {
                true
            }
            T::Collection(collection)
                if collection.kind != CollectionKind::Iterable && collection.non_empty =>
            {
                true
            }
            T::Shape(shape)
                if shape.kind == ShapeKind::Object
                    || shape.fields.iter().any(|field| !field.optional) =>
            {
                true
            }
            T::Shape(shape) if shape.fields.is_empty() && shape.remainder.is_none() => false,
            _ => return None,
        };
        if result.is_some_and(|existing| existing != value) {
            return None;
        }
        result = Some(value);
    }
    result
}
