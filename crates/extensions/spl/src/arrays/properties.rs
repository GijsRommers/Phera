//! An SPL array casts to its contents but debug output shows a private storage member.
//!
//! ```php
//! $a = new ArrayObject(['count' => 1], ArrayObject::ARRAY_AS_PROPS);
//! echo $a->count; // 1
//! var_dump($a);   // storage is displayed as a private member
//! ```

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, NativeError, NativePropertyAccess, NativePropertyMutation,
    NativePropertyRead, Object, PhpArray, PropertyPurpose, Value,
};

use super::dimensions;
use super::state::{self, Storage, Target};

pub(super) const ACCESS: NativePropertyAccess = NativePropertyAccess { read, mutate };

pub(super) fn view(
    _: &mut dyn Context,
    object: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    match purpose {
        PropertyPurpose::Debug => debug(object),
        PropertyPurpose::ArrayCast | PropertyPurpose::Export | PropertyPurpose::Json
            if !state::read(object, |state| state.flags.standard_properties())? =>
        {
            let target = Target::of(object)?;
            let storage = target.copy()?;
            if purpose == PropertyPurpose::ArrayCast && target.is_properties() {
                return storage
                    .iter()
                    .map(|(key, value)| {
                        Ok((ArrayKey::from_value(&Value::from(key))?, value.clone()))
                    })
                    .collect();
            }
            Ok(storage.as_ref().clone())
        }
        PropertyPurpose::Comparison => Ok([
            (ArrayKey::Int(0), Value::Array(Target::of(object)?.copy()?)),
            (ArrayKey::Int(1), Value::from(object.to_array())),
        ]
        .into_iter()
        .collect()),
        _ => Ok(object.to_array()),
    }
}

pub(super) fn debug(object: &Object) -> Result<PhpArray, NativeError> {
    let mut properties = object.to_array();
    let storage = state::read(object, |state| as_value(&state.storage))?;
    if let Some(storage) = storage {
        properties.insert(Object::private_key(state::base(object), "storage"), storage);
    }
    Ok(properties)
}

pub(super) fn as_value(storage: &Storage) -> Option<Value> {
    match storage {
        Storage::Array(array) => Some(Value::Array(array.clone())),
        Storage::Properties(object) | Storage::Alias(object) => Some(Value::Object(object.clone())),
        Storage::OwnProperties => None,
    }
}

pub(super) fn clone_state(original: &Rc<Object>, copy: &Object) -> Result<(), NativeError> {
    let mut state = state::read(original, Clone::clone)?;
    state.position = state::Position::Initial;
    state.sorting = false;
    state.storage = match state.storage {
        Storage::OwnProperties => Storage::OwnProperties,
        _ if state::base(original) == super::ITERATOR => Storage::Alias(original.clone()),
        _ => Storage::Array(Target::of(original)?.copy()?),
    };
    crate::store(copy, state)
}

fn use_backing(object: &Object, name: &str) -> Result<bool, NativeError> {
    let enabled = state::read(object, |state| {
        state.flags.array_as_properties() && !matches!(state.storage, Storage::OwnProperties)
    })?;
    Ok(enabled && !object.class.has_property(name) && !object.has(&ArrayKey::str(name)))
}

fn read(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
    mode: NativePropertyRead,
) -> Result<Option<Value>, NativeError> {
    if !use_backing(object, name)? {
        return Ok(None);
    }
    if matches!(mode, NativePropertyRead::Read) {
        return dimensions::get(context, object, Value::from(name)).map(Some);
    }
    let key = ArrayKey::from_value(&Value::from(name))?;
    Target::of(object)?.read(|array| {
        let value = array.get(&key).map(|value| value.deref().clone());
        match mode {
            NativePropertyRead::Exists => value,
            _ => Some(value.unwrap_or(Value::Null)),
        }
    })
}

fn mutate(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
    mutation: NativePropertyMutation<'_>,
) -> Result<bool, NativeError> {
    if !use_backing(object, name)? {
        return Ok(false);
    }
    let name = Value::from(name);
    match mutation {
        NativePropertyMutation::Assign(value) => {
            dimensions::set(context, object, name, value.clone())?;
        }
        NativePropertyMutation::Unset => dimensions::unset(context, object, name)?,
        _ => return Ok(false),
    }
    Ok(true)
}
