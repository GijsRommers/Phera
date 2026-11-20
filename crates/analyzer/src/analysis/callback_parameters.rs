//! PHP passes `array<string>` to the callback in `preg_replace_callback($re, $fn, $s)`.
//! A callable's documented signature can also type the parameters of an inline closure.

use super::*;
use crate::index::{ParameterInfo, Signature};
use crate::types::{Collection, CollectionKind};
use std::sync::Arc;

impl Analyzer<'_> {
    pub(super) fn callback_parameter_types(
        &self,
        label: &str,
        signature: Option<&Signature>,
        parameter: Option<&ParameterInfo>,
        previous: &[ValueType],
        arguments: &[Argument],
        scope: &Scope,
    ) -> Vec<ValueType> {
        if let Some(ValueType::Callable(callback)) = parameter.map(|parameter| &parameter.ty) {
            return callback
                .parameters
                .iter()
                .map(|parameter| parameter.ty.clone())
                .collect();
        }
        let Some((signature, parameter)) = signature.zip(parameter) else {
            return Vec::new();
        };
        if !signature.location.file.starts_with("ext/")
            && !signature.location.file.starts_with("Zend/")
        {
            return Vec::new();
        }
        use ValueType as T;
        match (label, parameter.name.as_str()) {
            ("set_error_handler()", "callback") => vec![T::Int, T::String, T::String, T::Int],
            ("preg_replace_callback()", "callback") => {
                let value = if arguments.len() > 5 {
                    T::String.join(&T::Null)
                } else {
                    T::String
                };
                vec![T::Collection(Arc::new(Collection {
                    kind: CollectionKind::Array,
                    key: T::Int.join(&T::String),
                    value,
                    non_empty: true,
                }))]
            }
            ("array_filter()", "callback") if arguments.len() < 3 => {
                vec![element_type(previous)]
            }
            ("array_map()", "callback") => {
                let [_, array] = arguments else {
                    return Vec::new();
                };
                let element = self.array_map_element(&array.value, scope);
                if element == T::Mixed {
                    Vec::new()
                } else {
                    vec![element]
                }
            }
            ("usort()" | "uasort()", "callback") => {
                let element = element_type(previous);
                vec![element.clone(), element]
            }
            _ => Vec::new(),
        }
    }

    // Array filtering and reindexing keep the element type. Inspect those
    // inputs without re-evaluating arguments or their side effects.
    fn array_map_element(&self, mut array: &Expression, scope: &Scope) -> ValueType {
        while let ExpressionKind::Call {
            callee,
            arguments: Arguments::List(arguments),
            ..
        } = &array.kind
            && let ExpressionKind::Name(name) = &callee.kind
            && self
                .builtin_predicate(name)
                .is_some_and(|name| matches!(name.as_str(), "array_filter" | "array_values"))
            && let Some(first) = arguments
                .first()
                .filter(|arg| !arg.spread && arg.name.is_none())
        {
            array = &first.value;
        }
        let Some(place) = crate::scope::Place::of(array) else {
            return ValueType::Mixed;
        };
        super::arrays::iteration_types(&self.place_type(&place, scope)).1
    }
}

fn element_type(previous: &[ValueType]) -> ValueType {
    previous
        .first()
        .map_or(ValueType::Mixed, |ty| super::arrays::iteration_types(ty).1)
}
