//! In `"Name: $value"`, mixed does not prove a safe conversion to a string.

use super::*;

impl Analyzer<'_> {
    pub(super) fn interpolated_string(
        &mut self,
        parts: &[StringPart],
        scope: &mut Scope,
        quiet: bool,
    ) -> ValueType {
        for part in parts {
            let (StringPart::Expression(expression) | StringPart::DollarCurly(expression, _)) =
                part
            else {
                continue;
            };
            let ty = self.expression(expression, scope, quiet);
            if ty == ValueType::Never {
                return ty;
            }
            if self.strict_mixed(&ty)
                && let ExpressionKind::Variable(Variable::Named(name)) = &expression.kind
            {
                self.diagnose(
                    "encapsedStringPart.nonString",
                    expression.span,
                    format!("Part ${name} (mixed) of encapsed string cannot be cast to string."),
                );
            }
        }
        ValueType::String
    }
}
