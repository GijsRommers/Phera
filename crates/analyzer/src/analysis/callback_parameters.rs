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
                let Some(place) = crate::scope::Place::of(&array.value) else {
                    return Vec::new();
                };
                let element = super::arrays::iteration_types(&self.place_type(&place, scope)).1;
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
}

fn element_type(previous: &[ValueType]) -> ValueType {
    previous
        .first()
        .map_or(ValueType::Mixed, |ty| super::arrays::iteration_types(ty).1)
}
