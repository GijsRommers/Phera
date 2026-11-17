//! Iterative copy-on-write reconstruction for nested PHP array and string writes.

use std::{cell::RefCell, rc::Rc};

use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Diagnostics, PhpArray, Value, to_php_string};

use crate::inline_stack::InlineStack;

/// Parents retain each detached container while an iterative write visits its child.
enum Parent {
    String {
        original: phera_value::PhpString,
        bytes: Vec<u8>,
        index: usize,
    },
    Array {
        array: Rc<PhpArray>,
        key: ArrayKey,
    },
    Reference(Rc<RefCell<Value>>),
    Object {
        object: Rc<phera_value::Object>,
        key: ArrayKey,
    },
}

/// Mutate a detached root in place and restore it before returning any failure.
pub(super) fn update(
    current: &mut Value,
    keys: &InlineStack<PathKey>,
    value: Value,
    mutation: crate::access_mode::Mutation,
    diagnostics: &mut dyn Diagnostics,
) -> Result<(), NativeError> {
    let mut parents = InlineStack::new();
    let result = walk(current, &mut parents, keys, value, mutation, diagnostics)
        .and_then(|()| rebuild(current, &mut parents, mutation, diagnostics));
    if result.is_err() {
        restore(current, &mut parents);
    }
    result
}

/// Validate each step before detaching its owner; no failure can lose the active child.
fn walk(
    current: &mut Value,
    parents: &mut InlineStack<Parent>,
    keys: &InlineStack<PathKey>,
    value: Value,
    mutation: crate::access_mode::Mutation,
    diagnostics: &mut dyn Diagnostics,
) -> Result<(), NativeError> {
    for key in keys.iter() {
        if let Value::Reference(reference) = current {
            let child = std::mem::take(
                &mut *reference
                    .try_borrow_mut()
                    .map_err(|_| internal_error!("borrowed reference"))?,
            );
            let Value::Reference(reference) = std::mem::replace(current, child) else {
                return Err(internal_error!("detached reference"));
            };
            parents.push(Parent::Reference(reference));
        }
        match key {
            PathKey::Property(key) => {
                let Value::Object(object) = current else {
                    return Err(NativeError::error(format!(
                        "Attempt to assign property \"{}\" on {}",
                        property_name(key),
                        current.type_name()
                    )));
                };
                let child = crate::property_storage::take(object, key);
                let Value::Object(object) = std::mem::replace(current, child) else {
                    return Err(internal_error!("detached object"));
                };
                parents.push(Parent::Object {
                    object,
                    key: key.clone(),
                });
            }
            PathKey::Index(key) => {
                if current.is_null() || matches!(current, Value::Bool(false)) {
                    *current = Value::Array(Rc::new(PhpArray::new()));
                }
                if let Value::String(string) = current {
                    let Some(parent) = prepare_string(string, key, &value, mutation, diagnostics)?
                    else {
                        return Ok(());
                    };
                    parents.push(parent);
                    *current = Value::Null;
                    continue;
                }
                let Value::Array(array) = current else {
                    let message = if let Value::Object(object) = current {
                        format!("Cannot use object of type {} as array", object.class.name())
                    } else {
                        "Cannot use a scalar value as an array".to_owned()
                    };
                    return Err(NativeError::error(message));
                };
                let key = match key {
                    Some(key) => key.clone(),
                    None => Rc::make_mut(array).push(Value::Null)?,
                };
                let child = Rc::make_mut(array)
                    .get_mut(&key)
                    .map(std::mem::take)
                    .unwrap_or_default();
                let Value::Array(array) = std::mem::take(current) else {
                    return Err(internal_error!("detached array"));
                };
                parents.push(Parent::Array { array, key });
                *current = child;
            }
        }
    }
    if let Value::Reference(reference) = current
        && mutation == crate::access_mode::Mutation::Assign
    {
        *reference
            .try_borrow_mut()
            .map_err(|_| internal_error!("borrowed reference"))? = value;
    } else {
        *current = value;
    }
    Ok(())
}

/// String offset writes retain PHP's warning and allocation-limit ordering.
fn prepare_string(
    string: &phera_value::PhpString,
    key: &Option<ArrayKey>,
    value: &Value,
    mutation: crate::access_mode::Mutation,
    diagnostics: &mut dyn Diagnostics,
) -> Result<Option<Parent>, NativeError> {
    let rebind = mutation != crate::access_mode::Mutation::Assign;
    let unset = mutation == crate::access_mode::Mutation::Unset;
    if unset {
        return Err(NativeError::error("Cannot unset string offsets"));
    }
    if rebind && matches!(value, Value::Reference(_)) {
        return Err(NativeError::error(
            "Cannot create references to/from string offsets",
        ));
    }
    let index = match key {
        Some(ArrayKey::Int(index)) => *index,
        Some(ArrayKey::String(string)) => {
            string.to_string_lossy().parse::<i64>().map_err(|_| {
                NativeError::type_error("Cannot access offset of type string on string")
            })?
        }
        None => return Err(NativeError::error("[] operator not supported for strings")),
    };
    let index = if index < 0 {
        string.len() as i64 + index
    } else {
        index
    };
    let Ok(index) = usize::try_from(index) else {
        diagnostics.warning(format!("Illegal string offset {index}"));
        return Ok(None);
    };
    let mut bytes = string.as_bytes().to_vec();
    if index > 16 * 1024 * 1024 {
        return Err(NativeError::error(
            "Phera string offset allocation limit exceeded",
        ));
    }
    if bytes.len() <= index {
        bytes.resize(index + 1, b' ')
    }
    Ok(Some(Parent::String {
        original: string.clone(),
        bytes,
        index,
    }))
}

/// Reattach parents; fallible string conversions retain the original parent for recovery.
fn rebuild(
    current: &mut Value,
    parents: &mut InlineStack<Parent>,
    mutation: crate::access_mode::Mutation,
    diagnostics: &mut dyn Diagnostics,
) -> Result<(), NativeError> {
    let mut remove = mutation == crate::access_mode::Mutation::Unset;
    while let Some(parent) = parents.pop() {
        let value = match parent {
            Parent::String {
                original,
                mut bytes,
                index,
            } => {
                let converted = to_php_string(current, diagnostics).map_err(NativeError::from);
                let string = match converted {
                    Ok(string) if !string.is_empty() => string,
                    result => {
                        parents.push(Parent::String {
                            original,
                            bytes,
                            index,
                        });
                        return Err(result.err().unwrap_or_else(|| {
                            NativeError::error("Cannot assign an empty string to a string offset")
                        }));
                    }
                };
                if string.len() > 1 {
                    diagnostics.warning(
                        "Only the first byte will be assigned to the string offset".into(),
                    );
                }
                if let (Some(slot), Some(byte)) = (bytes.get_mut(index), string.as_bytes().first())
                {
                    *slot = *byte;
                }
                Value::from(bytes)
            }
            Parent::Reference(reference) => {
                let result = reference
                    .try_borrow_mut()
                    .map(|mut slot| *slot = std::mem::take(current));
                if result.is_err() {
                    parents.push(Parent::Reference(reference));
                    return Err(internal_error!("borrowed reference"));
                }
                Value::Reference(reference)
            }
            Parent::Object { object, key } => {
                if remove {
                    object.remove(&key);
                    remove = false;
                } else {
                    crate::property_storage::write(&object, key, std::mem::take(current));
                }
                Value::Object(object)
            }
            Parent::Array { mut array, key } => {
                if remove {
                    Rc::make_mut(&mut array).remove(&key);
                    remove = false;
                } else {
                    Rc::make_mut(&mut array).insert(key, std::mem::take(current));
                }
                Value::Array(array)
            }
        };
        *current = value;
    }
    Ok(())
}

/// Recovery invokes no PHP conversions and walks parents without Rust recursion.
fn restore(current: &mut Value, parents: &mut InlineStack<Parent>) {
    while let Some(parent) = parents.pop() {
        *current = match parent {
            Parent::String { original, .. } => Value::String(original),
            Parent::Reference(reference) => {
                if let Ok(mut slot) = reference.try_borrow_mut() {
                    *slot = std::mem::take(current);
                }
                Value::Reference(reference)
            }
            Parent::Object { object, key } => {
                crate::property_storage::write(&object, key, std::mem::take(current));
                Value::Object(object)
            }
            Parent::Array { mut array, key } => {
                Rc::make_mut(&mut array).insert(key, std::mem::take(current));
                Value::Array(array)
            }
        };
    }
}

/// Property steps must never be confused with array offsets on scalar values.
#[derive(Clone)]
pub(crate) enum PathKey {
    Index(Option<ArrayKey>),
    Property(ArrayKey),
}

/// Diagnostics display the source property name rather than its storage prefix.
pub(crate) fn property_name(key: &ArrayKey) -> String {
    match key {
        ArrayKey::String(name) => {
            String::from_utf8_lossy(phera_value::split_property_key(name.as_bytes()).0).into_owned()
        }
        ArrayKey::Int(index) => index.to_string(),
    }
}
