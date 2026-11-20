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
            let key = key.map_or_else(|| "mixed".into(), ValueType::to_string);
            self.diagnose(
                "offsetAccess.nonOffsetAccessible",
                span,
                format!("Cannot access offset {key} on mixed."),
            );
        }
    }

    pub fn mixed_binary(
        &mut self,
        operator: BinaryOperator,
        left: &ValueType,
        right: &ValueType,
        span: Span,
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
        self.diagnose(
            "binaryOp.invalid",
            span,
            format!(
                "Binary operation \"{symbol}\" between {left} and {right} results in an error."
            ),
        );
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
