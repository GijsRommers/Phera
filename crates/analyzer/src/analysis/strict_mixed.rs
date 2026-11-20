//! Calling a method on `mixed $value` is reported from level 9.
//! Level 10 also checks a value from a parameter with no type declaration.

use super::*;

impl Analyzer<'_> {
    pub fn strict_mixed(&self, ty: &ValueType) -> bool {
        if self.level < 9 {
            return false;
        }
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            match ty {
                ValueType::ExplicitMixed if self.level >= 9 => return true,
                ValueType::ImplicitMixed if self.level >= 10 => return true,
                ValueType::Template(template) => pending.push(&template.bound),
                _ => {}
            }
        }
        false
    }
}

impl Analyzer<'_> {
    pub fn mixed_offset(&mut self, target: &ValueType, key: Option<&ValueType>, span: Span) {
        if self.strict_mixed(target) {
            let message = match key {
                Some(ValueType::OverflowingInt) => {
                    "Cannot access offset (float|int) on mixed.".into()
                }
                Some(key) => format!("Cannot access offset {key} on mixed."),
                None => "Cannot access an offset on mixed.".into(),
            };
            self.diagnose("offsetAccess.nonOffsetAccessible", span, message);
        }
    }

    pub fn mixed_binary(
        &mut self,
        operator: BinaryOperator,
        left: &ValueType,
        right: &ValueType,
        span: Span,
        compound: bool,
    ) {
        if !self.strict_mixed(left) && !self.strict_mixed(right) {
            return;
        }
        use BinaryOperator as Op;
        let symbol = match operator {
            Op::Add => "+",
            Op::Subtract => "-",
            Op::Multiply => "*",
            Op::Divide => "/",
            Op::Modulo => "%",
            Op::Power => "**",
            Op::Concat => ".",
            Op::BitwiseAnd => "&",
            Op::BitwiseOr => "|",
            Op::BitwiseXor => "^",
            Op::ShiftLeft => "<<",
            Op::ShiftRight => ">>",
            _ => return,
        };
        let (identifier, suffix) = if compound {
            ("assignOp.invalid", "=")
        } else {
            ("binaryOp.invalid", "")
        };
        self.diagnose(
            identifier,
            span,
            format!(
                "Binary operation \"{symbol}{suffix}\" between {left} and {right} results in an error."
            ),
        );
    }

    pub(super) fn mixed_increment(
        &mut self,
        ty: &ValueType,
        increment: bool,
        prefix: bool,
        span: Span,
    ) {
        if !self.strict_mixed(ty) {
            return;
        }
        let identifier = match (increment, prefix) {
            (true, true) => "preInc.type",
            (true, false) => "postInc.type",
            (false, true) => "preDec.type",
            (false, false) => "postDec.type",
        };
        let operator = if increment { "++" } else { "--" };
        self.diagnose(identifier, span, format!("Cannot use {operator} on mixed."));
    }
}

pub(super) fn accepts_mixed(ty: &ValueType) -> bool {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        if ty.is_mixed() {
            return true;
        }
        match ty {
            ValueType::Union(arms) => pending.extend(arms.iter()),
            ValueType::Template(template) => pending.push(&template.bound),
            _ => {}
        }
    }
    false
}
