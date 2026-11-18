//! `serialize($object)` uses `__serialize()`, or the `Serializable` interface or `__sleep()`.

use std::rc::Rc;

use phera_extension_api::serialization::{INCOMPLETE_CLASS, INCOMPLETE_CLASS_NAME};
use phera_extension_api::{ArrayKey, Context, NativeError, Object, PhpArray, Value};

use super::{Contents, Encoder, Identity, SerializationSlots};

impl Encoder {
    pub(super) async fn object(
        &mut self,
        context: &mut dyn Context,
        object: Rc<Object>,
    ) -> Result<(), NativeError> {
        let name = object.class.name();
        if context.serialization_forbidden(&object) {
            return Err(NativeError::throw(
                "Exception",
                format!("Serialization of '{name}' is not allowed"),
            ));
        }
        if context.has_serialization_method(&object, "__serialize") {
            let value = context
                .call_serialization_method(object.clone(), "__serialize", Vec::new())
                .await?;
            let Value::Array(entries) = value else {
                return Err(NativeError::type_error(format!(
                    "{name}::__serialize() must return an array"
                )));
            };
            self.output.object(name.as_bytes(), entries.len());
            self.entries(entries, Contents::Object);
        } else if object.class.is_instance_of("Serializable") {
            *context.state().get_or_default::<SerializationSlots>() =
                std::mem::take(&mut self.slots);
            let result = context
                .call_serialization_method(object.clone(), "serialize", Vec::new())
                .await;
            self.slots = context
                .state()
                .take::<SerializationSlots>()
                .unwrap_or_default();
            let result = result?;
            match &*result.try_deref()? {
                Value::String(payload) => self
                    .output
                    .custom_object(name.as_bytes(), payload.as_bytes()),
                Value::Null => {
                    let identity = Identity::Object(Rc::as_ptr(&object) as usize);
                    if let Some((slot, _)) = self.slots.identities.get_mut(&identity) {
                        *slot = 0;
                    }
                    self.output.null();
                }
                _ => {
                    return Err(NativeError::throw(
                        "Exception",
                        format!("{name}::serialize() must return a string or NULL"),
                    ));
                }
            }
        } else if context.has_serialization_method(&object, "__sleep") {
            let Some(entries) = sleep_properties(context, &object).await? else {
                self.output.null();
                return Ok(());
            };
            self.output.object(name.as_bytes(), entries.len());
            self.entries(Rc::new(entries), Contents::Object);
        } else {
            let mut entries = context.serialization_properties(&object)?;
            let original = if name == INCOMPLETE_CLASS {
                entries.remove(&ArrayKey::str(INCOMPLETE_CLASS_NAME))
            } else {
                None
            };
            let name = match &original {
                Some(Value::String(name)) => name.as_bytes(),
                _ => name.as_bytes(),
            };
            self.output.object(name, entries.len());
            self.entries(Rc::new(entries), Contents::Object);
        }
        Ok(())
    }
}

async fn sleep_properties(
    context: &mut dyn Context,
    object: &Rc<Object>,
) -> Result<Option<PhpArray>, NativeError> {
    let result = context
        .call_serialization_method(object.clone(), "__sleep", Vec::new())
        .await?;
    let invalid_return = || {
        format!(
            "serialize(): {}::__sleep() should return an array only containing \
         the names of instance-variables to serialize",
            object.class.name()
        )
    };
    let Value::Array(names) = result else {
        context.warning(invalid_return());
        return Ok(None);
    };
    let properties = context.serialization_properties(object)?;
    let mut selected = PhpArray::with_capacity(names.len());
    for name in names.values() {
        let name = name.try_deref()?.clone();
        if !matches!(name, Value::String(_)) {
            context.warning(invalid_return());
        }
        let name = context.stringify(&name).await?;
        let private = mangle(object.class.name().as_bytes(), name.as_bytes());
        let protected = mangle(b"*", name.as_bytes());
        let keys = [ArrayKey::str(name.clone()), private, protected];
        if let Some(key) = keys.into_iter().find(|key| {
            properties.contains_key(key)
                || context.has_uninitialized_serialized_property(object, key)
        }) {
            if let Some(value) = properties.get(&key)
                && selected.insert(key, value.clone()).is_some()
            {
                context.warning(format!(
                    "serialize(): \"{}\" is returned from __sleep() multiple times",
                    name.to_string_lossy()
                ));
            }
        } else {
            context.warning(format!(
                "serialize(): \"{}\" returned as member variable from __sleep() but does not exist",
                name.to_string_lossy()
            ));
        }
    }
    Ok(Some(selected))
}

fn mangle(scope: &[u8], name: &[u8]) -> ArrayKey {
    let mut key = Vec::with_capacity(scope.len() + name.len() + 2);
    key.push(0);
    key.extend_from_slice(scope);
    key.push(0);
    key.extend_from_slice(name);
    ArrayKey::str(key)
}
