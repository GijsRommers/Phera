//! In `$ready && true`, the right operand cannot affect the result.
//! The right operand is checked using the scope where PHP evaluates it.

use super::*;

#[derive(Clone, Copy)]
pub enum Side {
    Left,
    Right,
}

impl Analyzer<'_> {
    pub fn constant_logical_operand(
        &mut self,
        logical: &Expression,
        operand: &Expression,
        result: Option<bool>,
        side: Side,
    ) -> bool {
        let Some(value) = result else {
            return false;
        };
        if self.level < 4 {
            return false;
        }
        if self.type_test_call(operand) {
            return true;
        }
        if condition_has_own_rule(operand) {
            // The operand rule owns the operand. The combined result can
            // still be impossible because the tests contradict each other.
            return value;
        }
        if self.discarded_expression == Some(logical.span) && matches!(side, Side::Right) {
            return false;
        }
        let Some((kind, symbol)) = self.logical_name(logical) else {
            return false;
        };
        let (part, label) = match side {
            Side::Left => ("left", "Left"),
            Side::Right => ("right", "Right"),
        };
        if !value || self.last_condition != Some(logical.span) {
            let truth = if value { "True" } else { "False" };
            self.diagnose_dead_code(
                &format!("{kind}.{part}Always{truth}"),
                operand.span,
                format!("{label} side of {symbol} is always {value}."),
            );
        }
        true
    }

    pub fn constant_logical_result(&mut self, logical: &Expression, result: Option<bool>) {
        if self.level >= 4
            && self.discarded_expression != Some(logical.span)
            && let (Some(value), Some((kind, symbol))) = (result, self.logical_name(logical))
        {
            self.constant_result(
                kind,
                logical.span,
                value,
                format!("Result of {symbol} is always {value}."),
            );
        }
    }

    pub fn constant_negation(
        &mut self,
        expression: &Expression,
        operand: &Expression,
        value: Option<bool>,
    ) {
        if self.level >= 4
            && !condition_has_own_rule(operand)
            && !self.type_test_call(operand)
            && let Some(value) = value
            && (value || self.last_condition != Some(expression.span))
        {
            self.constant_result(
                "booleanNot",
                operand.span,
                !value,
                format!("Negated boolean expression is always {}.", !value),
            );
        }
    }

    fn logical_name(&self, logical: &Expression) -> Option<(&'static str, &'static str)> {
        let ExpressionKind::Binary {
            operator,
            left,
            right,
        } = &logical.kind
        else {
            return None;
        };
        let operator_source = self
            .file
            .source
            .get(left.span.end..right.span.start)
            .unwrap_or("")
            .trim();
        Some(match operator {
            BinaryOperator::And if operator_source.eq_ignore_ascii_case("and") => {
                ("logicalAnd", "and")
            }
            BinaryOperator::Or if operator_source.eq_ignore_ascii_case("or") => ("logicalOr", "or"),
            BinaryOperator::And => ("booleanAnd", "&&"),
            BinaryOperator::Or => ("booleanOr", "||"),
            BinaryOperator::Xor => ("logicalXor", "xor"),
            _ => return None,
        })
    }
}
