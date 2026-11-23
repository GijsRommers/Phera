//! A supplied int already satisfies the signature of `abs($integer)`.
//! String coercion and reference binding remain the responsibility of the normal parser.

use phera_bytecode::TypeMask;
use phera_extension_api::{MethodEntry, Param, ParamType};
use phera_value::Value;

/// The most arguments a method site runs a direct handler with.
pub(crate) const DIRECT_SITE_ARGUMENTS: usize = 3;

/// What each of a site's arguments may be for a direct handler to take it as
/// it is (see [`direct_site`]).
pub(crate) type DirectMasks = [TypeMask; DIRECT_SITE_ARGUMENTS];

/// The direct handler of `entry` for a method site that passes `arguments`
/// arguments, with what its parameters take as they are, or `None` when the
/// site must take the general way: no direct handler, a preparation of its
/// own, a by-reference or variadic parameter, too many or too few arguments.
///
/// The test [`accepts_direct`] makes on every call is made here once, when
/// the site resolves: a site passes the same number of arguments to the
/// same method every time, so a call has only each argument's own type left
/// to check ([`run_direct`]).
pub(crate) fn direct_site(
    entry: MethodEntry,
    arguments: usize,
) -> Option<(phera_extension_api::DirectMethod, DirectMasks)> {
    let handler = entry.direct?;
    let parameters = entry.params;
    if entry.preparation.is_some()
        || arguments > DIRECT_SITE_ARGUMENTS
        || arguments > parameters.len()
        || parameters
            .iter()
            .any(|param| param.by_ref || param.variadic)
        || parameters[arguments..].iter().any(|param| !param.optional)
    {
        return None;
    }
    let mut masks = [TypeMask::default(); DIRECT_SITE_ARGUMENTS];
    for (mask, parameter) in masks.iter_mut().zip(parameters) {
        *mask = direct_types(parameter);
    }
    Some((handler, masks))
}

/// Runs a direct handler on `object` with `arguments`, if each has a type
/// its parameter takes as it is. `None` when one does not or when the
/// handler declines, leaving everything as it was.
#[inline]
pub(crate) fn run_direct(
    (handler, masks): (phera_extension_api::DirectMethod, &DirectMasks),
    object: &phera_value::Object,
    arguments: &[Option<Value>],
) -> Option<Value> {
    let fits = arguments.iter().zip(masks).all(|(slot, mask)| {
        slot.as_ref()
            .is_some_and(|value| !value.is_reference() && mask.intersects(native_type(value)))
    });
    if fits {
        handler(object, arguments)
    } else {
        None
    }
}

/// What a parameter takes as it is, for a direct method: a `ReflectedObject`
/// takes any object, since the handler checks the reflected object's class
/// itself.
fn direct_types(parameter: &Param) -> TypeMask {
    match parameter.ty {
        ParamType::ReflectedObject if !parameter.by_ref && !parameter.variadic => TypeMask::OBJECT,
        _ => exact_types(parameter),
    }
}

/// These argument types are accepted as supplied. A zero mask reserves class,
/// callable and by-reference checks for the normal binder.
pub(super) fn accepts(parameters: &[Param], slots: &[Option<Value>]) -> bool {
    accepts_with(parameters, slots, exact_types)
}

/// For `new ReflectionFunction($closure)`, check the Closure arm without coercion.
pub(super) fn accepts_ready(parameters: &[Param], slots: &[Option<Value>]) -> bool {
    if let ([parameter], [Some(Value::Object(object))]) = (parameters, slots)
        && matches!(parameter.ty, ParamType::ClosureOrString)
    {
        return !parameter.by_ref && !parameter.variadic && object.class.name() == "Closure";
    }
    accepts(parameters, slots)
}

/// [`accepts`] for a direct method, which checks a reflected object's
/// class itself (and declines one of another class): a `ReflectedObject`
/// parameter takes any object here.
pub(super) fn accepts_direct(parameters: &[Param], slots: &[Option<Value>]) -> bool {
    accepts_with(parameters, slots, direct_types)
}

fn accepts_with(
    parameters: &[Param],
    slots: &[Option<Value>],
    types: impl Fn(&Param) -> TypeMask,
) -> bool {
    slots.len() <= parameters.len()
        && parameters[slots.len()..]
            .iter()
            .all(|parameter| parameter.optional)
        && parameters.iter().zip(slots).all(|(parameter, slot)| {
            slot.as_ref().is_some_and(|value| {
                !value.is_reference()
                    && !parameter.by_ref
                    && !parameter.variadic
                    && types(parameter).intersects(native_type(value))
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
        P::IntOrString | P::NativeNumeric(_) => T::INT.union(T::STRING),
        P::ArrayOrInt => T::ARRAY.union(T::INT),
        P::ArrayOrString => T::ARRAY.union(T::STRING),
        P::ObjectOrString => T::OBJECT.union(T::STRING),
        P::StringOrNumber => T::STRING.union(T::INT).union(T::FLOAT),
        P::ArrayOrNumber => T::ARRAY.union(T::INT).union(T::FLOAT),
        P::ArrayOrFloat => T::ARRAY.union(T::FLOAT),
        P::ArrayOrBool => T::ARRAY.union(T::BOOL),
        P::Untyped | P::Mixed => T::MIXED.union(RESOURCE),
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

#[cfg(test)]
mod tests {
    use super::*;

    const INT_AND_OPTIONAL_STRING: &[Param] = &[
        Param::new("id", ParamType::Int),
        Param::new("name", ParamType::String).optional(),
    ];
    const BY_REFERENCE: &[Param] = &[Param::new("id", ParamType::Int).by_ref()];

    fn entry(params: &'static [Param]) -> MethodEntry {
        MethodEntry::public("m", params, |_, _, _| Ok(Value::Null))
    }

    /// Just enough of a class for an object.
    struct Plain;

    impl phera_value::ClassHandle for Plain {
        fn name(&self) -> &str {
            "Plain"
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    fn double(_: &phera_value::Object, arguments: &[Option<Value>]) -> Option<Value> {
        match arguments {
            [Some(Value::Int(number)), ..] => Some(Value::Int(number * 2)),
            _ => None,
        }
    }

    /// A site takes a method's direct handler only for a number of
    /// arguments the parameters allow and only when none is by reference.
    #[test]
    fn a_direct_call_needs_a_handler_and_plain_parameters() {
        let direct = entry(INT_AND_OPTIONAL_STRING).with_direct(double);
        assert!(direct_site(entry(INT_AND_OPTIONAL_STRING), 1).is_none());
        assert!(direct_site(direct, 0).is_none());
        assert!(direct_site(direct, 1).is_some());
        assert!(direct_site(direct, 2).is_some());
        assert!(direct_site(direct, 3).is_none());
        assert!(direct_site(entry(BY_REFERENCE).with_direct(double), 1).is_none());
    }

    /// An argument of a type the parameter does not take as it is, and a
    /// reference, leave the call to the general way.
    #[test]
    fn a_direct_call_runs_only_arguments_the_parameters_take_as_they_are() {
        let object = phera_value::Object::new(1, std::rc::Rc::new(Plain));
        let (handler, masks) =
            direct_site(entry(INT_AND_OPTIONAL_STRING).with_direct(double), 2).unwrap();
        let call = |arguments: &[Option<Value>]| run_direct((handler, &masks), &object, arguments);
        let text = Value::from("a");
        assert!(matches!(
            call(&[Some(Value::Int(4)), Some(text.clone())]),
            Some(Value::Int(8))
        ));
        assert!(call(&[Some(text.clone()), Some(text.clone())]).is_none());
        assert!(call(&[Some(Value::Int(4)), Some(Value::Int(1))]).is_none());
        assert!(call(&[Some(Value::Int(4)), None]).is_none());
        let reference = Value::Reference(std::rc::Rc::new(std::cell::RefCell::new(Value::Int(4))));
        assert!(call(&[Some(reference), Some(text)]).is_none());
    }
}
