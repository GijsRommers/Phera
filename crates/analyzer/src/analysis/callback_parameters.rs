//! PHP passes `array<string>` to the callback in `preg_replace_callback($re, $fn, $s)`.
//! A callable's documented signature can also type the parameters of an inline closure.

use super::*;
use crate::index::{ParameterInfo, Signature};
use crate::scope::{AccessStep, Place};
use crate::types::{Collection, CollectionKind};
use std::sync::Arc;

impl Analyzer<'_> {
    pub(super) fn callback_parameter_types(
        &mut self,
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
                self.array_map_element(&array.value, scope)
                    .into_iter()
                    .collect()
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
    fn array_map_element(&mut self, mut array: &Expression, scope: &Scope) -> Option<ValueType> {
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
        let place = Place::of(array)?;
        let mut ty = self.place_type(&place, scope);
        let mut element = super::arrays::iteration_types(&ty).1;
        if element.is_mixed() && !place.members.is_empty() {
            let loaded = self.load_array_receiver_types(&place, scope);
            if !loaded.is_mixed() {
                ty = loaded;
                element = super::arrays::iteration_types(&ty).1;
            }
        }
        // An inferred collection with unknown elements already supplies the
        // callback context. It is different from a bare array declaration.
        (element != ValueType::Mixed || matches!(ty, ValueType::Collection(_))).then_some(element)
    }

    // The callback is processed before PHP evaluates its array argument.
    // A documented receiver class may still need its Composer declarations.
    fn load_array_receiver_types(&mut self, place: &Place, scope: &Scope) -> ValueType {
        let mut ty = self.place_type(&Place::variable(&place.root), scope);
        for member in &place.members {
            let (name, property) = match member {
                AccessStep::Property(name) => (name, true),
                AccessStep::Method(name, _) => (name, false),
                _ => return ValueType::Mixed,
            };
            for class in super::receiver_types::Receivers::of(&ty).classes {
                self.dependencies.resolve(self.index, class);
            }
            ty = self.place_member_type(&ty, name, property);
        }
        ty
    }
}

fn element_type(previous: &[ValueType]) -> ValueType {
    previous
        .first()
        .map_or(ValueType::Mixed, |ty| super::arrays::iteration_types(ty).1)
}
