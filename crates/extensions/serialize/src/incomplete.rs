//! `unserialize('O:7:"Missing":0:{}')` produces an `__PHP_Incomplete_Class`.

use phera_extension_api::serialization::{INCOMPLETE_CLASS, incomplete_object_message};
use phera_extension_api::{
    ClassEntry, ClassHooks, Context, NativeError, NativePropertyAccess, NativePropertyMutation,
    NativePropertyRead, Object, Value,
};

const ACCESS: NativePropertyAccess = NativePropertyAccess { read, mutate };
const HOOKS: ClassHooks = ClassHooks {
    property_access: Some(&ACCESS),
    ..ClassHooks::EMPTY
};

pub(crate) const CLASS: ClassEntry = ClassEntry {
    kind: phera_extension_api::ClassKind::FinalClass,
    hooks: Some(&HOOKS),
    ..ClassEntry::class(INCOMPLETE_CLASS)
};

fn read(
    context: &mut dyn Context,
    object: &Object,
    _: &str,
    mode: NativePropertyRead,
) -> Result<Option<Value>, NativeError> {
    if matches!(mode, NativePropertyRead::Update) {
        return Err(NativeError::error(incomplete_object_message(
            object,
            "modify a property on",
        )));
    }
    let function = if matches!(mode, NativePropertyRead::Exists) {
        "property_exists"
    } else {
        context.diagnostic_function().unwrap_or("Unknown")
    };
    context.warning(format!(
        "{function}(): {}",
        incomplete_object_message(object, "access a property on")
    ));
    if matches!(mode, NativePropertyRead::Exists) {
        return Ok(None);
    }
    Ok(Some(Value::Null))
}

fn mutate(
    _: &mut dyn Context,
    object: &Object,
    _: &str,
    _: NativePropertyMutation<'_>,
) -> Result<bool, NativeError> {
    Err(NativeError::error(incomplete_object_message(
        object,
        "modify a property on",
    )))
}
