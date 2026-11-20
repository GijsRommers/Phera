//! A `foreach (preg_split($pattern, $text) as $part)` must handle false.
//! Level 7 checks each known alternative of the iterable type.

use super::*;

impl Analyzer<'_> {
    pub(super) fn check_foreach_iterable(&mut self, ty: &ValueType, span: Span) {
        if self.level < 3 {
            return;
        }
        let bad = match ty {
            ValueType::Union(arms) => {
                if self.level >= 7 {
                    arms.iter().any(|arm| {
                        (self.level >= 8 || *arm != ValueType::Null) && non_iterable(arm)
                    })
                } else {
                    arms.iter().all(non_iterable)
                }
            }
            ty => self.strict_mixed(ty) || non_iterable(ty),
        };
        if bad {
            let name = ty
                .to_string()
                .replace("list<non-empty-string>", "list<string>");
            self.diagnose(
                "foreach.nonIterable",
                span,
                format!(
                    "Argument of an invalid type {name} supplied for foreach, only iterables are supported."
                ),
            );
        }
    }
}

fn non_iterable(ty: &ValueType) -> bool {
    use ValueType as T;
    matches!(
        ty,
        T::Null
            | T::Bool
            | T::BoolLiteral(_)
            | T::Int
            | T::IntLiteral(_)
            | T::IntRange(..)
            | T::OverflowingInt
            | T::Float
            | T::String
            | T::StringLiteral(_)
            | T::StringRefinement(_)
            | T::ClassString(_)
            | T::Resource
    )
}
