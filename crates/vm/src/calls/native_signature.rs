//! A supplied int already satisfies the signature of `abs($integer)`.
//! String coercion and reference binding remain the responsibility of the normal parser.

use phera_bytecode::TypeMask;
use phera_extension_api::{Param, ParamType};
use phera_value::Value;

/// These argument types are accepted as supplied. A zero mask reserves class,
/// callable and by-reference checks for the normal binder.
pub(super) fn accepts(parameters: &[Param], slots: &[Option<Value>]) -> bool {
    slots.len() <= parameters.len()
        && parameters[slots.len()..]
            .iter()
            .all(|parameter| parameter.optional)
        && parameters.iter().zip(slots).all(|(parameter, slot)| {
            slot.as_ref().is_some_and(|value| {
                !value.is_reference()
                    && !parameter.by_ref
                    && !parameter.variadic
                    && exact_types(parameter).intersects(native_type(value))
            })
        })
}

pub(super) fn exact_types(parameter: &Param) -> TypeMask {
    use ParamType as P;
    use TypeMask as T;
    if parameter.by_ref
        || parameter.variadic
        || matches!(parameter.ty, P::Callable | P::ReflectedObject)
    {
        return T::default();
    }
    let mask = match parameter.ty {
        P::Bool => T::BOOL,
        P::Int | P::RoundingModeOrInt => T::INT,
        P::Float => T::FLOAT,
        P::String | P::ClosureOrString | P::CertificateOrString => T::STRING,
        P::Array | P::Iterable => T::ARRAY,
        P::Object => T::OBJECT,
        P::ArrayOrObject | P::ObjectOrArray => T::ARRAY.union(T::OBJECT),
        P::Number => T::INT.union(T::FLOAT),
        P::IntOrString => T::INT.union(T::STRING),
        P::ArrayOrInt => T::ARRAY.union(T::INT),
        P::ArrayOrString => T::ARRAY.union(T::STRING),
        P::ObjectOrString => T::OBJECT.union(T::STRING),
        P::StringOrNumber => T::STRING.union(T::INT).union(T::FLOAT),
        P::ArrayOrNumber => T::ARRAY.union(T::INT).union(T::FLOAT),
        P::ArrayOrFloat => T::ARRAY.union(T::FLOAT),
        P::ArrayOrBool => T::ARRAY.union(T::BOOL),
        P::Mixed => T::MIXED.union(RESOURCE),
        P::Class(_) | P::Callable | P::ReflectedObject | P::ClassUnion(_) => T::default(),
    };
    if parameter.accepts_null() {
        mask.union(T::NULL)
    } else {
        mask
    }
}

const RESOURCE: TypeMask = TypeMask(1 << 13);

pub(super) fn native_type(value: &Value) -> TypeMask {
    match value {
        Value::Resource(_) => RESOURCE,
        value => crate::types::own_type(value),
    }
}
