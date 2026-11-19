//! Heap debug and serialization enumerate the internal binary heap order.
//!
//! ```php
//! var_dump($queue->__serialize());
//! ```

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, PropertyPurpose,
    Value, args,
};

use super::{Entry, Extract, Heap, PRIORITY_QUEUE, Status, operations::base, read, sift, update};
use crate::{This, receiver, store};

fn members(heap: &Heap, object: &Object) -> Value {
    let projection = if base(object) == PRIORITY_QUEUE {
        Extract::Both
    } else {
        Extract::Heap
    };
    crate::list(heap.items.iter().map(|entry| projection.project(entry)))
}

fn debug(object: &Object) -> Result<PhpArray, NativeError> {
    let mut properties = object.to_array();
    let base = base(object);
    read(object, |heap| {
        properties.insert(
            Object::private_key(base, "flags"),
            Value::Int(heap.projection(object).flags()),
        );
        properties.insert(
            Object::private_key(base, "isCorrupted"),
            Value::Bool(matches!(heap.status, Status::Corrupted)),
        );
        properties.insert(Object::private_key(base, "heap"), members(heap, object));
    })?;
    Ok(properties)
}

pub(super) fn view(
    _: &mut dyn Context,
    object: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    if purpose == PropertyPurpose::Debug {
        debug(object)
    } else {
        Ok(object.to_array())
    }
}

pub(super) fn debug_info(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    debug(receiver(this)?).map(Value::from)
}

pub(super) fn clone_heap(original: &Rc<Object>, copy: &Object) -> Result<(), NativeError> {
    store(copy, read(original, Clone::clone)?)
}

pub(super) fn serialize(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    read(object, |heap| {
        heap.validate(false)?;
        let state: PhpArray = [
            (
                ArrayKey::str("flags"),
                Value::Int(heap.projection(object).flags()),
            ),
            (ArrayKey::str("heap_elements"), members(heap, object)),
        ]
        .into_iter()
        .collect();
        Ok(crate::list([
            Value::from(object.to_array()),
            Value::from(state),
        ]))
    })?
}

pub(super) fn unserialize(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let invalid = || {
            NativeError::throw(
                "Exception",
                format!(
                    "Invalid serialization data for {} object",
                    object.class.name()
                ),
            )
        };
        let data = args::array(&arguments, 0)
            .filter(|data| data.len() == 2)
            .ok_or_else(invalid)?;
        let Some(Value::Array(properties)) = data.get(&ArrayKey::Int(0)) else {
            return Err(invalid());
        };
        object
            .update(|props| {
                for (key, value) in properties.iter() {
                    props.insert(key, value.clone());
                }
            })
            .ok_or_else(crate::busy)?;
        let Some(Value::Array(state)) = data.get(&ArrayKey::Int(1)) else {
            return Err(invalid());
        };
        let Some(Value::Int(flags)) = state.get(&ArrayKey::str("flags")) else {
            return Err(invalid());
        };
        let priority_queue = base(object) == PRIORITY_QUEUE;
        let flags = if priority_queue {
            Extract::from_flags(*flags).ok_or_else(invalid)?
        } else if *flags == 0 {
            Extract::Heap
        } else {
            return Err(invalid());
        };
        update(object, |heap| heap.extract = flags)?;
        let Some(Value::Array(entries)) = state.get(&ArrayKey::str("heap_elements")) else {
            return Err(invalid());
        };
        for value in entries.values() {
            let entry = if priority_queue {
                let Value::Array(pair) = value else {
                    return Err(invalid());
                };
                if pair.len() != 2 {
                    return Err(invalid());
                }
                let data = pair
                    .get(&ArrayKey::str("data"))
                    .ok_or_else(invalid)?
                    .clone();
                let priority = pair
                    .get(&ArrayKey::str("priority"))
                    .ok_or_else(invalid)?
                    .clone();
                Entry {
                    data,
                    priority: Some(priority),
                }
            } else {
                Entry {
                    data: value.clone(),
                    priority: None,
                }
            };
            sift::apply(context, object, sift::Change::Insert(entry)).await?;
        }
        Ok(Value::Null)
    })
}
