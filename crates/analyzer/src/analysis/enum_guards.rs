//! For a two-case enum, `$mode !== Mode::Read` implies `$mode === Mode::Write`.
//! Remember that implication for a readonly property across calls and branch joins.

use super::*;
use crate::scope::{GuardKey, Place};

impl Analyzer<'_> {
    pub(super) fn enum_case_class(&self, expression: &Expression) -> Option<String> {
        let ExpressionKind::ClassConstant {
            class,
            name: MemberName::Identifier(case),
        } = &expression.kind
        else {
            return None;
        };
        let ExpressionKind::Name(class) = &class.kind else {
            return None;
        };
        let candidates = self.context.names.resolve(class, UseKind::Class);
        let enumeration = self.index.class(candidates.first()?)?;
        (enumeration.kind == ClassKind::Enum && enumeration.enum_cases.contains(&case.name))
            .then(|| enumeration.name.clone())
    }

    pub fn enum_guard(&self, condition: &Expression, scope: &Scope) -> Option<(GuardKey, bool)> {
        let ExpressionKind::Binary {
            operator: operator @ (BinaryOperator::Identical | BinaryOperator::NotIdentical),
            left,
            right,
        } = &condition.kind
        else {
            return None;
        };
        let (tested, value) = if matches!(left.kind, ExpressionKind::ClassConstant { .. }) {
            (right.as_ref(), left.as_ref())
        } else {
            (left.as_ref(), right.as_ref())
        };
        let ExpressionKind::Property {
            target,
            name: MemberName::Identifier(property),
            nullsafe: false,
        } = &tested.kind
        else {
            return None;
        };
        let ExpressionKind::Variable(Variable::Named(receiver)) = &target.kind else {
            return None;
        };
        let ty = scope.get(receiver).ty;
        let owner = self.index.class(ty.class_name()?)?;
        let member = self.index.property(&owner.name, &property.name)?;
        if !owner.modifiers.is_readonly && !member.modifiers.is_readonly {
            return None;
        }
        let enum_name = member.ty.class_name()?;
        let enumeration = self.index.class(enum_name)?;
        if enumeration.kind != ClassKind::Enum {
            return None;
        }
        let ExpressionKind::ClassConstant {
            class,
            name: MemberName::Identifier(case),
        } = &value.kind
        else {
            return None;
        };
        let ExpressionKind::Name(class) = &class.kind else {
            return None;
        };
        let class = self
            .context
            .names
            .resolve(class, UseKind::Class)
            .into_iter()
            .next()?;
        if !class.eq_ignore_ascii_case(enum_name) {
            return None;
        }
        let mut cases = enumeration.enum_cases.iter();
        let first = cases.next()?;
        let second = cases.next()?;
        if cases.next().is_some() || (&case.name != first && &case.name != second) {
            return None;
        }
        let place = Place::of(tested)?;
        if place
            .members
            .iter()
            .any(|member| matches!(member, crate::scope::AccessStep::VariableOffset(_)))
        {
            return None;
        }
        let key = GuardKey::EnumCase {
            place,
            class: enumeration.name.clone(),
            case: first.clone(),
        };
        let inverted = (*operator == BinaryOperator::NotIdentical) != (&case.name == second);
        Some((key, inverted))
    }
}
