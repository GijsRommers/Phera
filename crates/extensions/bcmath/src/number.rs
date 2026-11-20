//! Immutable decimals. `(new BcMath\Number('12.00'))->value` preserves two places.

use std::rc::Rc;

use num_traits::Zero;

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassHooks, ClassKind, Context, NativeCastKind, NativeClone, NativeError,
    NativePropertyAccess, NativePropertyMutation, NativePropertyRead, NativeResult, Object,
    PhpArray, PropertyEntry, PropertyPurpose, PropertyType, Value, Visibility,
};

use crate::arithmetic::Result;
use crate::decimal::Decimal;

mod methods;
mod operators;

pub(crate) const NAME: &str = "BcMath\\Number";

pub(crate) const CLASS: ClassEntry = ClassEntry {
    kind: ClassKind::FinalReadonlyClass,
    interfaces: &["Stringable"],
    properties: &[
        PropertyEntry::typed("value", PropertyType::String, Visibility::Public).readonly(),
        PropertyEntry::typed("scale", PropertyType::Int, Visibility::Public).readonly(),
    ],
    methods: methods::METHODS,
    hooks: Some(&ClassHooks {
        clone: Some(NativeClone::copy::<Decimal>()),
        fork: Some(phera_extension_api::fork_clone::<Decimal>),
        compare: Some(operators::compare_objects),
        binary: Some(operators::binary),
        cast: Some(cast),
        properties: Some(properties),
        property_access: Some(&NativePropertyAccess { read, mutate }),
        ..ClassHooks::EMPTY
    }),
    ..ClassEntry::class(NAME)
};

pub(super) fn value(object: &Object) -> Option<Decimal> {
    object
        .native
        .borrow()
        .as_ref()?
        .downcast_ref::<Decimal>()
        .cloned()
}

pub(super) fn receiver(receiver: Option<&Rc<Object>>) -> Result<Decimal> {
    receiver
        .and_then(|object| value(object))
        .ok_or_else(|| NativeError::error("BcMath\\Number is not initialized"))
}

pub(super) fn create(context: &mut dyn Context, number: Decimal) -> NativeResult {
    let object = context.create_object(NAME, PhpArray::new())?;
    if let Value::Object(object) = &object {
        *object.native.borrow_mut() = Some(Box::new(number));
    }
    Ok(object)
}

pub(crate) fn pair(left: Value, right: Value) -> Value {
    let mut values = PhpArray::new();
    values.insert(ArrayKey::Int(0), left);
    values.insert(ArrayKey::Int(1), right);
    Value::from(values)
}

fn properties(_: &mut dyn Context, object: &Object, _: PropertyPurpose) -> Result<PhpArray> {
    let mut properties = PhpArray::new();
    if let Some(number) = value(object) {
        properties.insert(ArrayKey::str("value"), Value::from(number.render()));
        properties.insert(ArrayKey::str("scale"), Value::Int(i64::from(number.scale)));
    }
    Ok(properties)
}

fn read(
    _: &mut dyn Context,
    object: &Object,
    name: &str,
    _: NativePropertyRead,
) -> Result<Option<Value>> {
    let Some(number) = value(object) else {
        return Ok(None);
    };
    Ok(match name {
        "value" => Some(Value::from(number.render())),
        "scale" => Some(Value::Int(i64::from(number.scale))),
        _ => None,
    })
}

pub(super) fn readonly(property: &str) -> NativeError {
    NativeError::error(format!(
        "Cannot modify readonly property {NAME}::${property}"
    ))
}

fn mutate(
    _: &mut dyn Context,
    _: &Object,
    name: &str,
    mutation: NativePropertyMutation<'_>,
) -> Result<bool> {
    if !matches!(name, "value" | "scale") {
        return Ok(false);
    }
    match mutation {
        NativePropertyMutation::Unset => Err(NativeError::error(format!(
            "Cannot unset readonly property {NAME}::${name}"
        ))),
        NativePropertyMutation::Reference | NativePropertyMutation::Indirect => {
            Err(NativeError::error(format!(
                "Cannot indirectly modify readonly property {NAME}::${name}"
            )))
        }
        _ => Err(readonly(name)),
    }
}

fn cast(object: &Object, kind: NativeCastKind) -> Option<Value> {
    match kind {
        NativeCastKind::Bool => Some(Value::Bool(!value(object)?.coefficient.is_zero())),
        _ => None,
    }
}
