//! SplFixedArray has an explicit length; every unfilled slot holds null.
//!
//! ```php
//! $a = new SplFixedArray(2);
//! $a[1] = 'x';
//! echo json_encode($a); // [null,"x"]
//! ```

mod dimensions;
mod iteration;
mod registration;

pub(crate) use registration::register;

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, NativeError, NativeResult, Object, PhpArray, PropertyPurpose, Value, args,
};

use crate::{This, read_state, receiver, store, update_state};

const CLASS: &str = "SplFixedArray";

#[derive(Clone, Default)]
struct Fixed {
    slots: Vec<Value>,
}

fn read<T>(object: &Object, f: impl FnOnce(&Fixed) -> T) -> Result<T, NativeError> {
    read_state(object, f)
}

fn update<T>(object: &Object, f: impl FnOnce(&mut Fixed) -> T) -> Result<T, NativeError> {
    update_state(object, f)
}

fn size(value: i64, method: &str) -> Result<usize, NativeError> {
    usize::try_from(value).map_err(|_| {
        NativeError::value_error(format!(
            "{CLASS}::{method}(): Argument #1 ($size) must be greater than or equal to 0"
        ))
    })
}

fn resize(fixed: &mut Fixed, length: usize) -> Result<(), NativeError> {
    if length > fixed.slots.len() {
        fixed
            .slots
            .try_reserve_exact(length - fixed.slots.len())
            .map_err(|_| {
                NativeError::error(format!(
                    "Possible integer overflow in memory allocation ({length} * 16 + 0)"
                ))
            })?;
    }
    fixed.slots.resize(length, Value::Null);
    Ok(())
}

fn construct(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let length = size(args::int(arguments, 0).unwrap_or_default(), "__construct")?;
    update(receiver(this)?, |fixed| {
        if fixed.slots.is_empty() {
            resize(fixed, length)?;
        }
        Ok(Value::Null)
    })?
}

fn set_size(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let length = size(args::int(arguments, 0).unwrap_or_default(), "setSize")?;
    update(receiver(this)?, |fixed| resize(fixed, length))??;
    Ok(Value::Bool(true))
}

fn native_count(_: &mut dyn Context, object: &Object) -> Result<i64, NativeError> {
    read(object, |fixed| fixed.slots.len() as i64)
}

fn count(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    native_count(context, receiver(this)?).map(Value::Int)
}

fn to_array(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |fixed| {
        crate::list(fixed.slots.iter().cloned())
    })
}

fn from_array(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    let array = args::array(arguments, 0)
        .ok_or_else(|| NativeError::error("Missing SplFixedArray source array"))?;
    let preserve = args::bool(arguments, 1).unwrap_or(true);
    let mut fixed = Fixed::default();
    if preserve {
        let mut length = 0;
        for key in array.keys() {
            let ArrayKey::Int(key) = key else {
                return Err(invalid_keys());
            };
            let key = usize::try_from(key).map_err(|_| invalid_keys())?;
            if key >= i64::MAX as usize {
                return Err(NativeError::throw(
                    "InvalidArgumentException",
                    "integer overflow detected",
                ));
            }
            length = length.max(key + 1);
        }
        resize(&mut fixed, length)?;
        for (key, value) in array.iter() {
            if let ArrayKey::Int(key) = key
                && let Some(slot) = fixed.slots.get_mut(key as usize)
            {
                *slot = value.deref().clone();
            }
        }
    } else {
        fixed
            .slots
            .extend(array.values().map(|value| value.deref().clone()));
    }
    crate::make(context, CLASS, fixed).map(Value::Object)
}

fn invalid_keys() -> NativeError {
    NativeError::throw(
        "InvalidArgumentException",
        "array must contain only positive integer keys",
    )
}

fn clone_array(original: &Rc<Object>, copy: &Object) -> Result<(), NativeError> {
    store(copy, read(original, Clone::clone)?)
}

fn view(
    _: &mut dyn Context,
    object: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    match purpose {
        PropertyPurpose::ArrayCast
        | PropertyPurpose::Debug
        | PropertyPurpose::Export
        | PropertyPurpose::Comparison
        | PropertyPurpose::Serialize => combined(object),
        _ => Ok(object.to_array()),
    }
}

fn combined(object: &Object) -> Result<PhpArray, NativeError> {
    let properties = object.to_array();
    read(object, |fixed| {
        fixed
            .slots
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, value)| (ArrayKey::Int(index as i64), value))
            .chain(properties.iter().map(|(key, value)| (key, value.clone())))
            .collect()
    })
}

fn serialize(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    combined(receiver(this)?).map(Value::from)
}

fn unserialize(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let data = args::array(arguments, 0).ok_or_else(|| NativeError::error("Missing array data"))?;
    let mut values = Vec::with_capacity(data.len());
    let mut properties = PhpArray::new();
    for (key, value) in data.iter() {
        match key {
            ArrayKey::Int(_) => values.push(value.deref().clone()),
            ArrayKey::String(_) => {
                properties.insert(key, value.clone());
            }
        }
    }
    if read(object, |fixed| fixed.slots.is_empty())? {
        store(object, Fixed { slots: values })?;
        object
            .update(|props| {
                for (key, value) in properties.iter() {
                    props.insert(key, value.clone());
                }
            })
            .ok_or_else(crate::busy)?;
    }
    Ok(Value::Null)
}
