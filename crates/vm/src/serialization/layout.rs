//! The mangled wire key for `private $id` is the same for every instance.

use std::rc::Rc;

use phera_bytecode::ClassKind;
use phera_extension_api::serialization::{PlainSerializedClass, SerializedProperty};
use phera_value::ArrayKey;

use crate::{Vm, classes::Class};

pub(super) fn plain_layout(class: &Rc<Class>) -> Option<PlainSerializedClass> {
    let fields = class.serialized_fields.get_or_init(|| linked_fields(class));
    Some(PlainSerializedClass {
        class: class.clone(),
        fields: fields.as_ref()?.clone(),
    })
}

fn linked_fields(class: &Class) -> Option<Rc<[SerializedProperty]>> {
    if class.kind != ClassKind::Class
        || class.is_abstract
        || class.has_native_layout
        || class.property_template.is_none()
        || Vm::class_forbids_serialization(class)
        || class.is_a("Serializable")
        || Vm::serialization_method(class, "__unserialize").is_some()
        || Vm::serialization_method(class, "__wakeup").is_some()
    {
        return None;
    }
    class
        .defaults
        .iter()
        .map(|field| {
            let ArrayKey::String(name) = &field.key else {
                return None;
            };
            if field.ty.is_some() || !field.initialized {
                return None;
            }
            let mut token = format!("s:{}:\"", name.len()).into_bytes();
            token.extend_from_slice(name.as_bytes());
            token.extend_from_slice(b"\";");
            Some(SerializedProperty {
                key: field.key.clone(),
                token,
            })
        })
        .collect()
}
