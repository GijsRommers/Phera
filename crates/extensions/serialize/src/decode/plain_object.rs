//! A linked `class Item { public $id = 0; }` can receive its wire table in one step.

use std::rc::Rc;

use phera_extension_api::serialization::PlainSerializedClass;
use phera_extension_api::{Context, Object, PhpArray, Value};

use super::php_string;
use crate::wire::{Reader, Token};

pub(super) fn read(
    context: &mut dyn Context,
    reader: &mut Reader<'_>,
    layout: &PlainSerializedClass,
    count: usize,
) -> Option<Rc<Object>> {
    if count != layout.fields.len() {
        return None;
    }
    let mut probe = reader.clone();
    let mut properties = PhpArray::new();
    for field in layout.fields.iter() {
        if !probe.consume(&field.token) {
            return None;
        }
        let value = match probe.token(false).ok()? {
            Token::Null => Value::Null,
            Token::Bool(value) => Value::Bool(value),
            Token::Int(value) => Value::Int(value),
            Token::Float(value) => Value::Float(value),
            Token::String(value) => Value::String(php_string(value)),
            _ => return None,
        };
        if probe.take_diagnostic().is_some() {
            return None;
        }
        properties.insert(field.key.clone(), value);
    }
    probe.close().ok()?;
    let object = context.create_plain_unserialized_object(layout.class.as_ref(), properties)?;
    *reader = probe;
    Some(object)
}
