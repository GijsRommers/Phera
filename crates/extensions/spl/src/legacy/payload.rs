//! A legacy payload can contain another legacy object before its next value.
//!
//! For example, an ArrayObject's `C:` payload can hold a SplQueue's `C:`
//! payload. PHP fills the queue before continuing to read the array.

use std::rc::Rc;

use phera_extension_api::serialization::{
    ActiveLegacyUnserialize, ActiveUnserializePolicy, UnserializePolicy,
};
use phera_extension_api::{Context, NativeError, NativeResult, Object, Value};

use crate::{arrays, linked_list, object_storage};

use super::{
    custom::{custom_object, wrap_failure},
    read::{Reader, Step},
};

#[derive(Clone, Copy)]
pub(crate) enum PayloadKind {
    ArrayObject,
    ArrayIterator,
    LinkedList,
    ObjectStorage,
}

impl PayloadKind {
    pub(super) fn of(object: &Object) -> Option<Self> {
        let class = &object.class;
        if class.is_instance_of("ArrayObject") {
            Some(Self::ArrayObject)
        } else if class.is_instance_of("ArrayIterator") {
            Some(Self::ArrayIterator)
        } else if class.is_instance_of("SplDoublyLinkedList") {
            Some(Self::LinkedList)
        } else if class.is_instance_of("SplObjectStorage") {
            Some(Self::ObjectStorage)
        } else {
            None
        }
    }

    fn function(self) -> &'static str {
        match self {
            Self::ArrayObject => "ArrayObject::unserialize",
            Self::ArrayIterator => "ArrayIterator::unserialize",
            Self::LinkedList => "SplDoublyLinkedList::unserialize",
            Self::ObjectStorage => "SplObjectStorage::unserialize",
        }
    }
}

enum Field {
    ArrayFlags,
    ArrayStorage(i64),
    ListFlags,
    ListEntry,
    StorageCount,
    StorageItem(i64),
    StorageInfo { remaining: i64, item: Rc<Object> },
    Members,
    Done,
}

enum CustomValue<'a> {
    Inline(Payload<'a>),
    Object(Rc<Object>),
}

struct Payload<'a> {
    object: Rc<Object>,
    reader: Reader<'a>,
    field: Field,
}

impl<'a> Payload<'a> {
    fn new(
        object: Rc<Object>,
        input: &'a [u8],
        kind: PayloadKind,
        policy: UnserializePolicy,
    ) -> Result<Self, NativeError> {
        let mut reader = Reader::new(input, kind.function(), policy);
        let field = if reader.is_empty() {
            Field::Done
        } else {
            match kind {
                PayloadKind::ArrayObject | PayloadKind::ArrayIterator => {
                    arrays::legacy_check_mutable(&object)?;
                    reader.expect(b"x:")?;
                    Field::ArrayFlags
                }
                PayloadKind::LinkedList => {
                    linked_list::legacy_clear(&object)?;
                    Field::ListFlags
                }
                PayloadKind::ObjectStorage => {
                    reader.expect(b"x:")?;
                    Field::StorageCount
                }
            }
        };
        Ok(Self {
            object,
            reader,
            field,
        })
    }

    async fn accept(&mut self, context: &mut dyn Context, value: Value) -> Result<(), NativeError> {
        let field = std::mem::replace(&mut self.field, Field::Done);
        self.field = match (field, value) {
            (Field::ArrayFlags, Value::Int(flags)) => {
                if arrays::legacy_uses_properties(flags) {
                    arrays::legacy_set_storage(context, &self.object, flags, None)?;
                    self.members()?
                } else {
                    Field::ArrayStorage(flags)
                }
            }
            (Field::ArrayStorage(flags), value @ (Value::Array(_) | Value::Object(_))) => {
                arrays::legacy_set_storage(context, &self.object, flags, Some(value))?;
                self.reader.expect(b";")?;
                self.members()?
            }
            (Field::ListFlags, Value::Int(flags)) => {
                linked_list::legacy_set_mode(&self.object, flags)?;
                self.list_entry()?
            }
            (Field::ListEntry, value) => {
                linked_list::legacy_push(&self.object, value)?;
                self.list_entry()?
            }
            (Field::StorageCount, Value::Int(count)) if count >= 0 => self.storage_item(count)?,
            (Field::StorageItem(remaining), Value::Object(item)) => {
                if self.reader.take(b",") {
                    Field::StorageInfo { remaining, item }
                } else {
                    self.attach(context, remaining, item, Value::Null).await?
                }
            }
            (Field::StorageInfo { remaining, item }, value) => {
                self.attach(context, remaining, item, value).await?
            }
            (Field::Members, Value::Array(members)) => {
                self.object
                    .update(|props| {
                        for (key, value) in members.iter() {
                            props.insert(key, value.clone());
                        }
                    })
                    .ok_or_else(crate::busy)?;
                Field::Done
            }
            _ => return Err(self.reader.error()),
        };
        Ok(())
    }

    fn members(&mut self) -> Result<Field, NativeError> {
        self.reader.expect(b"m:")?;
        Ok(Field::Members)
    }

    fn list_entry(&mut self) -> Result<Field, NativeError> {
        if self.reader.take(b":") {
            Ok(Field::ListEntry)
        } else if self.reader.is_empty() {
            Ok(Field::Done)
        } else {
            Err(self.reader.error())
        }
    }

    fn storage_item(&mut self, count: i64) -> Result<Field, NativeError> {
        if count == 0 {
            self.members()
        } else {
            Ok(Field::StorageItem(count))
        }
    }

    async fn attach(
        &mut self,
        context: &mut dyn Context,
        remaining: i64,
        item: Rc<Object>,
        info: Value,
    ) -> Result<Field, NativeError> {
        self.reader.expect(b";")?;
        object_storage::legacy_attach(context, &self.object, item, info).await?;
        self.storage_item(remaining - 1)
    }
}

pub(crate) async fn decode(
    context: &mut dyn Context,
    object: Rc<Object>,
    input: &[u8],
    kind: PayloadKind,
) -> NativeResult {
    let policy = ActiveUnserializePolicy::current(context);
    let mut payload = Payload::new(object, input, kind, policy)?;
    let mut parents = Vec::new();
    let slots = ActiveLegacyUnserialize::replace(context, None);
    let from_outer_decoder = slots.is_some();
    let mut references = slots.unwrap_or_default();
    let mut completed = None;
    let decoded = async {
        loop {
            if matches!(payload.field, Field::Done) {
                let Some((parent, _)) = parents.pop() else {
                    return Ok(());
                };
                completed = Some(Value::Object(payload.object));
                payload = parent;
            }
            match payload
                .reader
                .value(context, &mut references, completed.take())
                .await?
            {
                Step::Value(value) => payload.accept(context, value).await?,
                Step::Custom(custom) => {
                    let offset = custom.offset;
                    let child = async {
                        let (object, kind) =
                            custom_object(context, &mut references, &custom).await?;
                        if let Some(kind) = kind {
                            Ok(CustomValue::Inline(Payload::new(
                                object,
                                custom.payload,
                                kind,
                                custom.policy,
                            )?))
                        } else {
                            Ok(CustomValue::Object(object))
                        }
                    }
                    .await
                    .map_err(|error| wrap_failure(context, &payload.reader, offset, error))?;
                    match child {
                        CustomValue::Inline(child) => {
                            parents.push((std::mem::replace(&mut payload, child), offset));
                        }
                        CustomValue::Object(object) => completed = Some(Value::Object(object)),
                    }
                }
            }
        }
    }
    .await;
    if let Err(mut error) = decoded {
        while let Some((parent, offset)) = parents.pop() {
            error = wrap_failure(context, &parent.reader, offset, error);
        }
        return Err(error);
    }
    if from_outer_decoder {
        ActiveLegacyUnserialize::replace(context, Some(references));
    } else if !references.hooks.is_empty() {
        let previous = ActiveUnserializePolicy::replace(context, None);
        let result = async {
            for hook in references.hooks {
                hook.invoke(context).await?;
            }
            Ok::<_, NativeError>(())
        }
        .await;
        ActiveUnserializePolicy::replace(context, previous);
        result?;
    }
    Ok(Value::Null)
}
