//! Serialization stores flat object/info pairs while debug output names each pair.
//!
//! ```php
//! var_dump($storage->__serialize(), $storage->__debugInfo());
//! ```

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeComparison, NativeError, NativeResult, Object, PhpArray,
    PropertyPurpose, Value, args,
    value::{Diagnostics, OpError},
};

use super::{CLASS, Key, attach, hash, read};
use crate::{
    This,
    legacy::{self, PayloadKind, Writer},
    receiver,
};

pub(super) fn compare(
    left: &Object,
    right: &Object,
    _: &mut dyn Diagnostics,
) -> Result<NativeComparison, OpError> {
    if right.class.name() == CLASS && left.class.name() == CLASS {
        Ok(NativeComparison::Default)
    } else {
        Ok(NativeComparison::Uncomparable)
    }
}

fn debug(object: &Object) -> Result<PhpArray, NativeError> {
    let mut props = object.to_array();
    let storage = read(object, |state| {
        state
            .entries
            .values()
            .enumerate()
            .map(|(i, entry)| {
                let pair: PhpArray = [
                    (ArrayKey::str("obj"), entry.object.clone()),
                    (ArrayKey::str("inf"), entry.info.clone()),
                ]
                .into_iter()
                .collect();
                (ArrayKey::Int(i as i64), Value::from(pair))
            })
            .collect::<PhpArray>()
    })?;
    props.insert(Object::private_key(CLASS, "storage"), Value::from(storage));
    Ok(props)
}

pub(super) fn view(
    _: &mut dyn Context,
    object: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    match purpose {
        PropertyPurpose::Debug => debug(object),
        PropertyPurpose::Comparison => read(object, |data| {
            data.entries
                .iter()
                .map(|(key, entry)| {
                    let key = match key {
                        Key::Handle(handle) => ArrayKey::Int(i64::from(*handle)),
                        Key::Custom(hash) => ArrayKey::String(hash.clone()),
                    };
                    (key, entry.info.clone())
                })
                .collect()
        }),
        _ => Ok(object.to_array()),
    }
}

pub(super) fn debug_info(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    debug(receiver(this)?).map(Value::from)
}

pub(super) fn serialize(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let contents = read(object, |data| {
        data.entries
            .values()
            .flat_map(|entry| [entry.object.clone(), entry.info.clone()])
            .enumerate()
            .map(|(index, value)| (ArrayKey::Int(index as i64), value))
            .collect::<PhpArray>()
    })?;
    Ok(Value::from(
        [
            (ArrayKey::Int(0), Value::from(contents)),
            (ArrayKey::Int(1), Value::from(object.to_array())),
        ]
        .into_iter()
        .collect::<PhpArray>(),
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
        let (Some(Value::Array(storage)), Some(Value::Array(members))) = (field(0), field(1))
        else {
            return Err(invalid("Incomplete or ill-typed serialization data"));
        };
        if storage.len() % 2 != 0 {
            return Err(invalid("Odd number of elements"));
        }
        let mut values = storage.values();
        while let (Some(key), Some(info)) = (values.next(), values.next()) {
            let Value::Object(key) = key else {
                return Err(invalid("Non-object key"));
            };
            let hash = hash(context, object, key).await?;
            attach(object, hash, key.clone(), info.deref().clone())?;
        }
        object
            .update(|props| {
                for (key, value) in members.iter() {
                    props.insert(key, value.clone());
                }
            })
            .ok_or_else(crate::busy)?;
        Ok(Value::Null)
    })
}

fn invalid(message: &str) -> NativeError {
    NativeError::throw("UnexpectedValueException", message)
}

pub(super) fn legacy_serialize(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let mut writer = Writer::default();
    writer.bytes(b"x:");
    read(object, |data| {
        writer.value(Value::Int(data.entries.len() as i64))?;
        for entry in data.entries.values() {
            writer.value(entry.object.clone())?;
            writer.bytes(b",");
            writer.value(entry.info.clone())?;
            writer.bytes(b";");
        }
        Ok::<_, NativeError>(())
    })??;
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
        let input = args::string(&arguments, 0)
            .ok_or_else(|| NativeError::error("Missing serialized storage data"))?;
        legacy::decode(
            context,
            object.clone(),
            input.as_bytes(),
            PayloadKind::ObjectStorage,
        )
        .await
    })
}
