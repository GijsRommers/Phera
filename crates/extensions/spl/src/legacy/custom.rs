//! A `C:` token resolves its class and runs the Serializable method immediately.

use std::rc::Rc;

use phera_extension_api::serialization::{
    ActiveUnserializePolicy, INCOMPLETE_CLASS, INCOMPLETE_CLASS_NAME, LegacyUnserializeState,
    resolve_unserialize_class,
};
use phera_extension_api::{ArrayKey, Context, NativeError, Object, Value};

use super::{
    payload::PayloadKind,
    read::{Custom, Reader},
};

pub(super) async fn custom_object(
    context: &mut dyn Context,
    references: &mut LegacyUnserializeState,
    custom: &Custom<'_>,
) -> Result<(Rc<Object>, Option<PayloadKind>), NativeError> {
    let info = resolve_unserialize_class(context, custom.class, &custom.policy).await?;
    let name = info
        .as_ref()
        .map_or(INCOMPLETE_CLASS, |class| class.name.as_str());
    if info.as_ref().is_some_and(|class| class.forbidden) {
        return Err(NativeError::throw(
            "Exception",
            format!("Unserialization of '{name}' is not allowed"),
        ));
    }
    let has_unserializer = info.as_ref().is_some_and(|class| class.custom);
    if !has_unserializer {
        context.warning(format!("Class {name} has no unserializer"));
    }
    let Value::Object(object) = context.create_unserialized_object(name)? else {
        return Err(NativeError::error(
            "Unserialization requires a constructed object",
        ));
    };
    references.objects.insert(custom.slot, object.clone());
    if info.is_none() {
        object.set(
            ArrayKey::str(INCOMPLETE_CLASS_NAME),
            Value::from(custom.class),
        );
    }
    if !has_unserializer {
        return Ok((object, None));
    }
    if !context.is_user_method(&object, "unserialize")
        && let Some(kind) = PayloadKind::of(&object)
    {
        return Ok((object, Some(kind)));
    }
    let previous = ActiveUnserializePolicy::replace(context, Some(custom.policy.clone()));
    let result = context
        .call_serialization_method(
            object.clone(),
            "unserialize",
            vec![Value::from(custom.payload)],
        )
        .await;
    ActiveUnserializePolicy::replace(context, previous);
    result?;
    Ok((object, None))
}

/// PHP wraps a failed nested custom deserializer at the start of its payload.
pub(super) fn wrap_failure(
    context: &mut dyn Context,
    parent: &Reader<'_>,
    offset: usize,
    error: NativeError,
) -> NativeError {
    if context.is_exception(&error) {
        context.chain_exception(parent.error_at(offset), error)
    } else {
        error
    }
}
