//! Iterative copy-on-write reconstruction for nested PHP array and string writes.

use std::{cell::RefCell, rc::Rc};

use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, Diagnostics, PhpArray, Value, to_php_string};

use crate::inline_stack::InlineStack;

enum Parent {
    String(Vec<u8>, usize),
    Array(Rc<PhpArray>, ArrayKey),
    Reference(Rc<RefCell<Value>>),
}
pub(super) fn update(
    mut current: Value,
    keys: &InlineStack<Option<ArrayKey>>,
    value: Value,
    rebind: bool,
    unset: bool,
    diagnostics: &mut dyn Diagnostics,
) -> NativeResult {
    let mut parents = InlineStack::new();
    for key in keys.iter() {
        if let Value::Reference(register) = current {
            let value = std::mem::take(
                &mut *register
                    .try_borrow_mut()
                    .map_err(|_| NativeError::error("Phera internal error: borrowed reference"))?,
            );
            parents.push(Parent::Reference(register));
            current = value;
        }
        if current.is_null() || matches!(current, Value::Bool(false)) {
            current = Value::Array(Rc::new(PhpArray::new()))
        }
        if let Value::String(string) = &current {
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
                return Ok(current);
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
            parents.push(Parent::String(bytes, index));
            current = Value::Null;
            continue;
        }
        let Value::Array(mut array) = current else {
            return Err(NativeError::error("Cannot use a scalar value as an array"));
        };
        let key = if let Some(key) = key {
            key.clone()
        } else {
            let array = Rc::make_mut(&mut array);
            array.push(Value::Null)?
        };
        current = Rc::make_mut(&mut array)
            .get_mut(&key)
            .map(std::mem::take)
            .unwrap_or_default();
        parents.push(Parent::Array(array, key));
    }
    if let Value::Reference(register) = current {
        if !rebind {
            *register
                .try_borrow_mut()
                .map_err(|_| NativeError::error("Phera internal error: borrowed reference"))? =
                value;
            current = Value::Reference(register)
        } else {
            current = value
        }
    } else {
        current = value
    }
    let mut remove = unset;
    while let Some(parent) = parents.pop() {
        current = match parent {
            Parent::String(mut bytes, index) => {
                let string = to_php_string(&current, diagnostics)?;
                let Some(b) = string.as_bytes().first() else {
                    return Err(NativeError::error(
                        "Cannot assign an empty string to a string offset",
                    ));
                };
                if string.len() > 1 {
                    diagnostics
                        .warning("Only the first byte will be assigned to the string offset".into())
                }
                if let Some(slot) = bytes.get_mut(index) {
                    *slot = *b;
                }
                Value::from(bytes)
            }
            Parent::Reference(register) => {
                *register.try_borrow_mut().map_err(|_| {
                    NativeError::error("Phera internal error: borrowed reference")
                })? = current;
                Value::Reference(register)
            }
            Parent::Array(mut array, key) => {
                if remove {
                    Rc::make_mut(&mut array).remove(&key);
                    remove = false
                } else {
                    Rc::make_mut(&mut array).insert(key, current);
                }
                Value::Array(array)
            }
        };
    }
    Ok(current)
}
