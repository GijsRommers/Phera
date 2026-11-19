//! Modern serialization separates native storage from ordinary class properties.
//!
//! ```php
//! $a = new ArrayObject([7]);
//! var_dump($a->__serialize()); // [0, [7], [], null]
//! ```

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, Value, args,
};
use std::rc::Rc;

use super::{
    methods::storage,
    properties,
    state::{self, Flags, Position, Storage},
};
use crate::{
    This,
    legacy::{Reader, Writer},
    receiver,
};

const SELF_STORAGE_FLAG: i64 = 0x0100_0000;

pub(super) fn serialize(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let (flags, storage, iterator) = state::read(object, |state| {
        let flags = state.flags.raw()
            | if matches!(state.storage, Storage::OwnProperties) {
                SELF_STORAGE_FLAG
            } else {
                0
            };
        (
            flags,
            properties::as_value(&state.storage),
            state.iterator_class.clone(),
        )
    })?;
    Ok(Value::from(
        [
            Value::Int(flags),
            storage.unwrap_or(Value::Null),
            Value::from(object.to_array()),
            iterator.map_or(Value::Null, Value::from),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, value)| (ArrayKey::Int(index as i64), value))
        .collect::<phera_extension_api::PhpArray>(),
    ))
}

pub(super) fn unserialize(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let field =
            |index| args::array(&arguments, 0).and_then(|data| data.get(&ArrayKey::Int(index)));
        let (Some(Value::Int(flags)), Some(contents), Some(Value::Array(members))) =
            (field(0), field(1), field(2))
        else {
            return Err(invalid_serialization());
        };
        let iterator = match field(3) {
            Some(Value::String(name)) => Some(name.to_string_lossy()),
            None | Some(Value::Null) => None,
            _ => return Err(invalid_serialization()),
        };
        let storage = if flags & SELF_STORAGE_FLAG != 0 {
            Storage::OwnProperties
        } else {
            storage(context, object, contents, "__unserialize")?
        };
        let restored_flags = match &storage {
            Storage::Alias(alias) => state::read(alias, |s| s.flags)?,
            _ => Flags::new(*flags),
        };
        state::update(object, |state| {
            state.storage = storage;
            state.flags = restored_flags;
            state.position = Position::Initial;
            Ok(())
        })?;
        object
            .update(|target| {
                for (key, value) in members.iter() {
                    target.insert(key, value.clone());
                }
            })
            .ok_or_else(crate::busy)?;
        if let Some(name) = iterator {
            let class = context.lookup_class(&name).await?.ok_or_else(|| {
                NativeError::throw(
                    "UnexpectedValueException",
                    format!(
                        "Cannot deserialize ArrayObject with iterator class '{name}'; \
                     no such class exists"
                    ),
                )
            })?;
            if !class.is_instance_of("Iterator") {
                return Err(NativeError::throw(
                    "UnexpectedValueException",
                    format!(
                        "Cannot deserialize ArrayObject with iterator class '{name}'; \
                     this class does not implement the Iterator interface"
                    ),
                ));
            }
            state::update(object, |state| {
                state.iterator_class = Some(class.name().to_owned());
                Ok(())
            })?;
        }
        Ok(Value::Null)
    })
}

fn invalid_serialization() -> NativeError {
    NativeError::throw(
        "UnexpectedValueException",
        "Incomplete or ill-typed serialization data",
    )
}

pub(super) fn legacy_serialize(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let (flags, value) = state::read(object, |state| {
        (state.flags, properties::as_value(&state.storage))
    })?;
    let flags = flags.raw()
        | if value.is_none() {
            SELF_STORAGE_FLAG
        } else {
            0
        };
    let mut writer = Writer::default();
    writer.bytes(b"x:");
    writer.value(Value::Int(flags))?;
    if let Some(value) = value {
        writer.value(value)?;
        writer.bytes(b";");
    }
    writer.bytes(b"m:");
    writer.value(Value::from(object.to_array()))?;
    Ok(writer.finish())
}

pub(super) fn legacy_unserialize(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let input = args::string(&arguments, 0).ok_or_else(invalid_serialization)?;
        let mut reader = Reader::new(
            context,
            input.as_bytes(),
            if object.class.is_instance_of("ArrayObject") {
                "ArrayObject"
            } else {
                "ArrayIterator"
            },
        );
        if reader.is_empty() {
            return Ok(Value::Null);
        }
        state::check_mutable(object)?;
        reader.expect(b"x:")?;
        let Value::Int(flags) = reader.value(context)? else {
            return Err(reader.error());
        };
        let storage = if flags & SELF_STORAGE_FLAG != 0 {
            Storage::OwnProperties
        } else {
            let value = reader.value(context)?;
            if !matches!(value, Value::Array(_) | Value::Object(_)) {
                return Err(reader.error());
            }
            let storage = storage(context, object, &value, "unserialize")?;
            reader.expect(b";")?;
            storage
        };
        state::update(object, |state| {
            state.storage = storage;
            state.flags = Flags::new(flags);
            state.position = Position::Initial;
            Ok(())
        })?;
        reader.expect(b"m:")?;
        let Value::Array(members) = reader.value(context)? else {
            return Err(reader.error());
        };
        object
            .update(|props| {
                for (key, value) in members.iter() {
                    props.insert(key, value.clone());
                }
            })
            .ok_or_else(crate::busy)?;
        reader.finish(context).await?;
        Ok(Value::Null)
    })
}
