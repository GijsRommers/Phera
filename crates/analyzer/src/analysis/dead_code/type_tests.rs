//! `$item instanceof Child` may succeed for a `Parent` parameter.
//! Class ancestry proves inclusion, and unrelated concrete classes prove disjointness.

use super::{comparisons::*, *};
use crate::types::{Compatibility, TypePolicy};
use ValueType as T;

impl Analyzer<'_> {
    pub fn type_test_call(&self, expression: &Expression) -> bool {
        matches!(&expression.kind, ExpressionKind::Call { callee, .. }
            if matches!(&callee.kind, ExpressionKind::Name(name)
                if self.builtin_predicate(name).as_deref()
                    .and_then(super::super::refine_types::predicate_type).is_some()))
    }

    pub fn constant_instanceof(
        &mut self,
        value: &Expression,
        given: &T,
        class: String,
        scope: &Scope,
        span: Span,
    ) -> Option<T> {
        if self.trait_diagnostics.is_some()
            && matches!(&value.kind, ExpressionKind::Variable(Variable::Named(name)) if name == "this")
        {
            return None;
        }
        // Class tests against optional dependencies cannot establish a result.
        self.index.class(&class)?;
        let tested = T::Class(class);
        let result = uniform_pairs(given, &tested, |given, tested| {
            test(given, tested, self.index)
        })?;
        let provenance = self.narrowed_at(value, scope);
        self.constant_result(
            "instanceof",
            span,
            result,
            format!(
                "Instanceof between {given} and {tested} will always evaluate to {result}.{provenance}"
            ),
        );
        Some(T::BoolLiteral(result))
    }

    pub fn constant_type_test(
        &mut self,
        function: &str,
        arguments: &Arguments,
        types: &[T],
        scope: &Scope,
        span: Span,
    ) -> Option<T> {
        let expected = super::super::refine_types::predicate_type(function)?;
        let (Arguments::List(args), [given]) = (arguments, types) else {
            return None;
        };
        let [argument] = args.as_slice() else {
            return None;
        };
        if argument.spread {
            return None;
        }
        let result = uniform_pairs(given, &expected, |given, expected| {
            test(given, expected, self.index)
        })?;
        let provenance = self.narrowed_at(&argument.value, scope);
        let identifier = if result {
            "function.alreadyNarrowedType"
        } else {
            "function.impossibleType"
        };
        let has_assignment = matches!(
            argument.value.kind,
            ExpressionKind::Assign { .. }
                | ExpressionKind::AssignReference { .. }
                | ExpressionKind::CompoundAssign { .. }
        );
        if !result
            || !has_assignment
                && self
                    .last_condition
                    .is_none_or(|condition| condition.start != span.start)
        {
            self.diagnose_dead_code(
                identifier,
                span,
                format!("Call to function {function}() with {given} will always evaluate to {result}.{provenance}"),
            );
        }
        Some(T::BoolLiteral(result))
    }
}

fn test(given: &T, tested: &T, index: &Index) -> Option<bool> {
    use Compatibility::*;
    if tested == &T::Resource && given == &T::Resource {
        // A closed resource is still stored in the same variable.
        return None;
    }
    if tested == &T::UnknownCallable {
        return match category(given) {
            Some(
                Category::Int
                | Category::Float
                | Category::Bool
                | Category::Null
                | Category::Resource,
            ) => Some(false),
            _ if matches!(given, T::Callable(_)) => Some(true),
            _ => None,
        };
    }
    if tested == &T::Iterable {
        return match tested.accepts(given, index, TypePolicy::Documentation) {
            Accepts => Some(true),
            Rejects if category(given).is_some_and(|kind| kind != Category::Object) => Some(false),
            _ => None,
        };
    }
    let expected = category(tested)?;
    if expected != category(given)? {
        return Some(false);
    }
    if let Some(tested) = tested.class_name() {
        return instance_of(given, tested, index);
    }
    Some(true)
}

fn instance_of(given: &T, tested: &str, index: &Index) -> Option<bool> {
    let from = if matches!(given, T::Callable(_)) {
        index.class("Closure")?
    } else {
        index.class(given.class_name()?)?
    };
    let to = index.class(tested)?;
    let includes = |base: &str, child: &str| {
        index
            .ancestry(child)
            .iter()
            .any(|info| info.name.eq_ignore_ascii_case(base))
    };
    if includes(&to.name, &from.name) {
        return Some(true);
    }
    if includes(&from.name, &to.name) || !index.complete(&from.name) || !index.complete(&to.name) {
        return None;
    }
    let can_share_subclass = if from.kind == ClassKind::Interface {
        to.kind == ClassKind::Interface || to.kind == ClassKind::Class && !to.modifiers.is_final
    } else {
        to.kind == ClassKind::Interface && from.kind == ClassKind::Class && !from.modifiers.is_final
    };
    (!can_share_subclass).then_some(false)
}
