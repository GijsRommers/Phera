//! `strlen([])` passes an array to a string parameter. A first-class
//! callable and a named argument still carry the declaration's contract.

use super::*;
use crate::index::{ParameterInfo, Signature};
use crate::types::{Collection, CollectionKind, ShapeKey, ShapeKind};

impl Analyzer<'_> {
    pub fn check_argument_contracts(
        &mut self,
        arguments: &[Argument],
        types: &[ValueType],
        signature: &Signature,
        target: &str,
        scope: &Scope,
    ) {
        if self.level < 5 {
            return;
        }
        let mut position = 0;
        let mut uncertain_position = false;
        for (argument, given) in arguments.iter().zip(types) {
            if argument.spread {
                if let ValueType::Shape(shape) = given
                    && shape.kind != ShapeKind::Object
                    && shape.remainder.is_none()
                    && shape.fields.iter().all(|field| !field.optional)
                {
                    for field in &shape.fields {
                        let name = match &field.key {
                            ShapeKey::String(name) => Some(name.as_ref()),
                            ShapeKey::Integer(_) if !uncertain_position => None,
                            ShapeKey::Integer(_) => continue,
                        };
                        self.check_bound_argument(
                            signature,
                            target,
                            (position, name),
                            argument,
                            &field.value,
                            scope,
                        );
                        position += usize::from(name.is_none());
                    }
                } else {
                    uncertain_position = true;
                }
                continue;
            }
            let name = argument.name.as_ref().map(|name| name.name.as_str());
            if name.is_some() || !uncertain_position {
                self.check_bound_argument(
                    signature,
                    target,
                    (position, name),
                    argument,
                    given,
                    scope,
                );
            }
            position += usize::from(name.is_none());
        }
    }

    fn check_bound_argument(
        &mut self,
        signature: &Signature,
        target: &str,
        binding: (usize, Option<&str>),
        argument: &Argument,
        given: &ValueType,
        scope: &Scope,
    ) {
        let (position, name) = binding;
        let Some(parameter) = parameter(signature, position, name) else {
            return;
        };
        let bind = match name {
            Some(_) => format!("Parameter ${}", parameter.name),
            None => format!("Parameter #{} ${}", position + 1, parameter.name),
        };
        let reference_output = creates_reference_output(target, parameter);
        if !reference_output && self.incompatible(&parameter.ty, given, self.value_policy()) {
            let given = type_for_message(given, &parameter.ty);
            let expected = &parameter.ty;
            let (expected, given) = if self.level >= 7 {
                (
                    super::member_checks::receiver_name(expected),
                    super::member_checks::receiver_name(&given),
                )
            } else {
                (expected.to_string(), given.to_string())
            };
            let provenance = self.narrowed_at(&argument.value, scope);
            self.diagnose(
                "argument.type",
                argument.span,
                format!("{bind} of {target} expects {expected}, {given} given.{provenance}"),
            );
        }
        if parameter.by_reference && !argument.spread && !self.reference_argument(&argument.value) {
            self.diagnose(
                "argument.byRef",
                argument.span,
                format!("{bind} of {target} is passed by reference, so it expects variables only."),
            );
        }
        let static_closure = match &argument.value.kind {
            ExpressionKind::Closure(value) => value.is_static,
            ExpressionKind::ArrowFunction(value) => value.is_static,
            _ => false,
        };
        if parameter.closure_receiver.is_some() && static_closure {
            self.diagnose(
                "argument.staticClosure",
                argument.span,
                format!("{bind} of {target} expects bindable closure, static closure given."),
            );
        }
    }

    fn reference_argument(&self, mut expression: &Expression) -> bool {
        loop {
            match &expression.kind {
                ExpressionKind::Variable(_) | ExpressionKind::StaticProperty { .. } => return true,
                ExpressionKind::Property {
                    target,
                    nullsafe: false,
                    ..
                }
                | ExpressionKind::Index { target, .. } => expression = target,
                ExpressionKind::Call { callee, .. } => {
                    let ExpressionKind::Name(name) = &callee.kind else {
                        return false;
                    };
                    return self
                        .context
                        .names
                        .resolve(name, UseKind::Function)
                        .iter()
                        .find_map(|name| self.index.functions.get(&name.to_ascii_lowercase()))
                        .is_some_and(|signature| signature.returns_reference);
                }
                ExpressionKind::MethodCall {
                    nullsafe: false, ..
                }
                | ExpressionKind::StaticCall { .. } => {
                    // The receiver may dispatch to a method returning by reference.
                    return true;
                }
                _ => return false,
            }
        }
    }
}

pub(super) fn select_target(signature: &Signature, label: &str, kind: CallKind) -> String {
    if label == "callable" || label == "Closure::__invoke()" {
        return "closure".into();
    }
    if matches!(kind, CallKind::Callable) {
        return label.into();
    }
    if let Some(class) = label.strip_prefix("new ") {
        format!("class {class} constructor")
    } else if label.contains("::") {
        let prefix = if signature.modifiers.is_static {
            "static method"
        } else {
            "method"
        };
        format!("{prefix} {label}")
    } else {
        format!("function {}", label.trim_end_matches("()"))
    }
}

fn parameter<'a>(
    signature: &'a Signature,
    position: usize,
    name: Option<&str>,
) -> Option<&'a ParameterInfo> {
    match name {
        Some(name) => signature
            .parameters
            .iter()
            .find(|parameter| parameter.name == name),
        None => signature.parameters.get(position),
    }
    .or_else(|| {
        signature
            .parameters
            .last()
            .filter(|parameter| parameter.variadic)
    })
}

fn creates_reference_output(target: &str, parameter: &ParameterInfo) -> bool {
    parameter.by_reference
        && matches!(
            (target, parameter.name.as_str()),
            ("function preg_match" | "function preg_match_all", "matches")
                | ("function parse_str" | "function mb_parse_str", "result")
                | ("function str_replace" | "function str_ireplace", "count")
                | ("function exec", "output" | "result_code")
                | ("function system" | "function passthru", "result_code")
                | ("function proc_open", "pipes")
                | ("function flock", "would_block")
                | (
                    "function stream_socket_client" | "function stream_socket_server",
                    "error_code" | "error_message"
                )
        )
}

fn type_for_message(given: &ValueType, expected: &ValueType) -> ValueType {
    use ValueType as T;
    match (given, expected) {
        (T::Shape(_), T::Shape(_)) => given.clone(),
        (T::Shape(shape), T::Collection(expected)) if shape.kind != ShapeKind::Object => {
            let (key, value) = super::arrays::iteration_types(given);
            T::Collection(std::sync::Arc::new(Collection {
                kind: if expected.kind == CollectionKind::List {
                    CollectionKind::List
                } else {
                    CollectionKind::Array
                },
                key: scalar_for_message(&key, &expected.key),
                value: scalar_for_message(&value, &expected.value),
                non_empty: false,
            }))
        }
        (T::Shape(shape), _) if shape.kind != ShapeKind::Object => T::Array,
        _ => scalar_for_message(given, expected),
    }
}

fn scalar_for_message(given: &ValueType, expected: &ValueType) -> ValueType {
    use ValueType as T;
    match (given, expected) {
        (T::StringLiteral(_), T::StringLiteral(_) | T::StringRefinement(_) | T::ClassString(_))
        | (T::IntLiteral(_), T::IntLiteral(_) | T::IntRange(..)) => given.clone(),
        (T::IntLiteral(_), _) => T::Int,
        (T::StringLiteral(_), _) => T::String,
        _ => given.clone(),
    }
}
