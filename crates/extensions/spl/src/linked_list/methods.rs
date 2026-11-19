//! Push and pop use the back of the list; unshift and shift use the front.
//!
//! ```php
//! $queue->enqueue($job);
//! $job = $queue->dequeue();
//! ```

use phera_extension_api::{
    ArrayKey, Context, NativeError, NativeResult, Object, PhpArray, PropertyPurpose, Value, args,
};
use std::rc::Rc;

use super::{CLASS, List, Mode, dimensions::arg, range, read, update};
use crate::{
    This,
    legacy::{Reader, Writer},
    receiver,
};

pub(super) fn push(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    push_value(receiver(this)?, arg(args, 0))
}

fn push_value(object: &Object, value: Value) -> NativeResult {
    update(object, |list| {
        let entry = list.entry(value);
        list.entries.push_back(entry);
        Value::Null
    })
}

pub(super) fn push_direct(object: &Object, args: &[Option<Value>]) -> Option<Value> {
    let value = args.first()?.as_ref()?.deref().clone();
    push_value(object, value).ok()
}

pub(super) fn pop_direct(object: &Object, _: &[Option<Value>]) -> Option<Value> {
    take(object, End::Back).ok().flatten()
}

pub(super) fn shift_direct(object: &Object, _: &[Option<Value>]) -> Option<Value> {
    take(object, End::Front).ok().flatten()
}

#[derive(Clone, Copy)]
enum End {
    Front,
    Back,
}

fn take(object: &Object, end: End) -> Result<Option<Value>, NativeError> {
    let value = crate::native_state::update_present_state(object, |list: &mut List| {
        match end {
            End::Front => list.entries.pop_front(),
            End::Back => list.entries.pop_back(),
        }
        .map(|entry| entry.value)
    })?;
    Ok(value.flatten())
}

pub(super) fn unshift(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    update(receiver(this)?, |list| {
        let entry = list.entry(arg(args, 0));
        list.entries.push_front(entry);
        Value::Null
    })
}

pub(super) fn pop(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    take(receiver(this)?, End::Back)?.ok_or_else(|| empty("pop from"))
}

pub(super) fn shift(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    take(receiver(this)?, End::Front)?.ok_or_else(|| empty("shift from"))
}

pub(super) fn top(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |list| {
        list.entries.back().map(|entry| entry.value.clone())
    })?
    .ok_or_else(|| empty("peek at"))
}

pub(super) fn bottom(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |list| {
        list.entries.front().map(|entry| entry.value.clone())
    })?
    .ok_or_else(|| empty("peek at"))
}

fn empty(action: &str) -> NativeError {
    NativeError::throw(
        "RuntimeException",
        format!("Can't {action} an empty datastructure"),
    )
}

pub(super) fn add(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let index = usize::try_from(args::int(args, 0).unwrap_or(-1)).map_err(|_| range("add"))?;
    update(object, |list| {
        let slot = if index == list.entries.len() {
            index
        } else {
            list.slot(index, list.mode(object).order)
                .ok_or_else(|| range("add"))?
        };
        let value = list.entry(arg(args, 1));
        list.entries.insert(slot, value);
        Ok(Value::Null)
    })?
}

pub(super) fn native_count(_: &mut dyn Context, object: &Object) -> Result<i64, NativeError> {
    read(object, |list| list.entries.len() as i64)
}

pub(super) fn count(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    native_count(context, receiver(this)?).map(Value::Int)
}

pub(super) fn is_empty(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |list| Value::Bool(list.entries.is_empty()))
}

pub(super) fn get_mode(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    read(object, |list| Value::Int(list.mode(object).raw()))
}

pub(super) fn set_mode(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let requested = args::int(args, 0).unwrap_or_default();
    update(object, |list| {
        let mode = list.mode(object);
        if mode.raw() & 4 != 0 && mode.order != Mode::parse(requested).order {
            return Err(NativeError::throw(
                "RuntimeException",
                "Iterators' LIFO/FIFO modes for SplStack/SplQueue objects are frozen",
            ));
        }
        let mode = Mode::parse((requested & 3) | (mode.raw() & 4));
        list.mode = Some(mode);
        Ok(Value::Int(mode.raw()))
    })?
}

pub(super) fn clone_list(original: &Rc<Object>, copy: &Object) -> Result<(), NativeError> {
    let mut list = read(original, Clone::clone)?;
    list.cursor = super::Position::default();
    crate::store(copy, list)
}

fn elements(list: &List) -> Value {
    crate::list(list.entries.iter().map(|entry| entry.value.clone()))
}

fn debug(object: &Object) -> Result<PhpArray, NativeError> {
    let mut output = object.to_array();
    read(object, |list| {
        output.insert(
            Object::private_key(CLASS, "flags"),
            Value::Int(list.mode(object).raw()),
        );
        output.insert(Object::private_key(CLASS, "dllist"), elements(list));
    })?;
    Ok(output)
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

pub(super) fn serialize(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    read(object, |list| {
        crate::list([
            Value::Int(list.mode(object).raw()),
            elements(list),
            Value::from(object.to_array()),
        ])
    })
}

pub(super) fn unserialize(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let field = |index| args::array(args, 0).and_then(|data| data.get(&ArrayKey::Int(index)));
    let (Some(Value::Int(flags)), Some(Value::Array(items)), Some(Value::Array(members))) =
        (field(0), field(1), field(2))
    else {
        return Err(NativeError::throw(
            "UnexpectedValueException",
            "Incomplete or ill-typed serialization data",
        ));
    };
    update(object, |list| {
        list.mode = Some(Mode::parse(i64::from(*flags as i32)));
        for value in items.values() {
            let entry = list.entry(value.clone());
            list.entries.push_back(entry);
        }
    })?;
    object
        .update(|props| {
            for (key, value) in members.iter() {
                props.insert(key, value.clone());
            }
        })
        .ok_or_else(crate::busy)?;
    Ok(Value::Null)
}

pub(super) fn legacy_serialize(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let mut writer = Writer::default();
    read(object, |list| {
        writer.value(Value::Int(list.mode(object).raw()))?;
        for entry in &list.entries {
            writer.bytes(b":");
            writer.value(entry.value.clone())?;
        }
        Ok::<_, NativeError>(())
    })??;
    Ok(writer.finish())
}

pub(super) fn legacy_unserialize(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    let object = receiver(this)?;
    let input =
        args::string(args, 0).ok_or_else(|| NativeError::error("Missing serialized list data"))?;
    let mut reader = Reader::new(input.as_bytes());
    if reader.is_empty() {
        return Ok(Value::Null);
    }
    update(object, |list| list.entries.clear())?;
    let Value::Int(flags) = reader.value(context)? else {
        return Err(reader.error());
    };
    update(object, |list| {
        list.mode = Some(Mode::parse(i64::from(flags as i32)))
    })?;
    while reader.take(b":") {
        let value = reader.value(context)?;
        update(object, |list| {
            let entry = list.entry(value);
            list.entries.push_back(entry);
        })?;
    }
    if !reader.is_empty() {
        return Err(reader.error());
    }
    Ok(Value::Null)
}
