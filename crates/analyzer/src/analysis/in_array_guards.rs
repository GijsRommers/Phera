//! `in_array($mode, [Mode::READ, Mode::WRITE], true)` can narrow the mode to int.
//! The false path removes only fully known literal alternatives.

use super::*;

impl Analyzer<'_> {
    pub(super) fn refine_in_array(
        &self,
        arguments: &[Argument],
        scope: &mut Scope,
        truth: bool,
        span: Span,
    ) {
        let [subject, list, strict, ..] = arguments else {
            return;
        };
        if !matches!(
            strict.value.kind,
            ExpressionKind::Literal(Literal::Bool(true))
        ) {
            return;
        }
        let ExpressionKind::Array(items) = &list.value.kind else {
            return;
        };
        let mut possibilities = ValueType::Never;
        for item in items {
            let Some(value) = self.membership_value(&item.value, truth) else {
                return;
            };
            if item.spread {
                return;
            }
            possibilities = possibilities.join(&value);
        }
        if possibilities != ValueType::Never {
            self.refine_type(&subject.value, possibilities, scope, truth, span);
        }
    }

    fn membership_value(&self, expression: &Expression, truth: bool) -> Option<ValueType> {
        use ValueType as T;
        if let Some(literal) = crate::constant_types::literal(expression) {
            return Some(literal);
        }
        let constant = match &expression.kind {
            ExpressionKind::ClassConstant {
                class,
                name: MemberName::Identifier(name),
            } => {
                let ExpressionKind::Name(class) = &class.kind else {
                    return None;
                };
                if ["self", "static", "parent"]
                    .iter()
                    .any(|name| class.text.eq_ignore_ascii_case(name))
                {
                    return None;
                }
                let class = self.context.names.resolve(class, UseKind::Class);
                self.index.constant(&class[0], &name.name)
            }
            ExpressionKind::Name(name) => self
                .context
                .names
                .resolve(name, UseKind::Constant)
                .iter()
                .find_map(|name| self.index.constants.get(name)),
            _ => return None,
        }?;
        let ty = constant.literal.as_ref().unwrap_or(&constant.ty);
        match ty {
            T::IntLiteral(_) | T::StringLiteral(_) | T::BoolLiteral(_) | T::Null => {
                Some(ty.clone())
            }
            T::Int | T::Float | T::String | T::Bool if truth => Some(ty.clone()),
            _ => None,
        }
    }
}
