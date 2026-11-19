//! Serialize primitive values, arrays and ordinary objects in PHP's wire format.
//!
//! ```php
//! $list->push(['a' => 1]);
//! $list->serialize();
//! ```

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use phera_extension_api::{ArrayKey, NativeError, Object, Value, value::float_repr};

#[derive(Default)]
pub(crate) struct Writer {
    bytes: Vec<u8>,
    sequence: usize,
    objects: HashMap<*const Object, usize>,
    references: HashMap<*const RefCell<Value>, usize>,
}

enum Part {
    Value(Value),
    Key(ArrayKey),
    Close,
}

impl Writer {
    pub fn bytes(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub fn finish(self) -> Value {
        Value::from(self.bytes)
    }

    pub fn value(&mut self, value: Value) -> Result<(), NativeError> {
        let mut stack = vec![Part::Value(value)];
        while let Some(part) = stack.pop() {
            match part {
                Part::Value(value) => self.write_value(value, &mut stack)?,
                Part::Key(key) => match key {
                    ArrayKey::Int(key) => self.bytes(format!("i:{key};").as_bytes()),
                    ArrayKey::String(key) => self.string(key.as_bytes()),
                },
                Part::Close => self.bytes(b"}"),
            }
        }
        Ok(())
    }

    fn string(&mut self, bytes: &[u8]) {
        self.bytes(format!("s:{}:\"", bytes.len()).as_bytes());
        self.bytes(bytes);
        self.bytes(b"\";");
    }

    fn write_value(&mut self, value: Value, pending: &mut Vec<Part>) -> Result<(), NativeError> {
        let value = if let Value::Reference(reference) = &value {
            let identity = Rc::as_ptr(reference);
            if let Some(number) = self.references.get(&identity) {
                self.bytes(format!("R:{number};").as_bytes());
                return Ok(());
            }
            self.references
                .insert(identity, self.sequence.saturating_add(1));
            value.deref().clone()
        } else {
            value
        };
        self.sequence = self.sequence.saturating_add(1);
        match value {
            Value::Null => self.bytes(b"N;"),
            Value::Bool(value) => self.bytes(if value { b"b:1;" } else { b"b:0;" }),
            Value::Int(value) => self.bytes(format!("i:{value};").as_bytes()),
            Value::Float(value) => self.bytes(format!("d:{};", float_repr(value)).as_bytes()),
            Value::String(string) => self.string(string.as_bytes()),
            Value::Resource(_) => self.bytes(b"i:0;"),
            Value::Array(array) => {
                self.bytes(format!("a:{}:{{", array.len()).as_bytes());
                Self::members(&array, pending);
            }
            Value::Object(object) => {
                let identity = Rc::as_ptr(&object);
                if let Some(number) = self.objects.get(&identity) {
                    self.bytes(format!("r:{number};").as_bytes());
                    return Ok(());
                }
                if object.class.has_native_properties()
                    || object.class.is_instance_of("WeakReference")
                    || object.class.is_instance_of("Closure")
                {
                    return Err(NativeError::throw(
                        "Exception",
                        format!(
                            "Serialization of '{}' is not supported by SPL legacy serialization",
                            object.class.name(),
                        ),
                    ));
                }
                self.objects.insert(identity, self.sequence);
                let name = object.class.name();
                let properties = object.to_array();
                self.bytes(
                    format!("O:{}:\"{name}\":{}:{{", name.len(), properties.len()).as_bytes(),
                );
                Self::members(&properties, pending);
            }
            Value::Reference(_) => {
                return Err(NativeError::error(
                    "Cannot serialize a cyclic reference cell",
                ));
            }
        }
        Ok(())
    }

    fn members(array: &phera_extension_api::PhpArray, pending: &mut Vec<Part>) {
        pending.push(Part::Close);
        let start = pending.len();
        for (key, value) in array.iter() {
            pending.push(Part::Key(key));
            pending.push(Part::Value(value.clone()));
        }
        pending[start..].reverse();
    }
}
