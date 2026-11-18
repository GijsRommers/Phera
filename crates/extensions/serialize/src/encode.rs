//! `serialize([$object, $object])` emits `r:2;` for the second object occurrence.

mod objects;

use std::{collections::HashMap, rc::Rc};

use phera_extension_api::serialization::SerializationSettings;
use phera_extension_api::value::float_repr_with_precision;
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, PhpArray, Value};

use crate::wire::Writer;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum Identity {
    Reference(usize),
    Object(usize),
}

struct Frame {
    entries: Rc<PhpArray>,
    position: Option<usize>,
    kind: Contents,
}

#[derive(Clone, Copy)]
enum Contents {
    Array,
    Object,
}

#[derive(Default)]
struct Encoder {
    output: Writer,
    frames: Vec<Frame>,
    identities: HashMap<Identity, (usize, Value)>,
    slots: usize,
}

pub async fn serialize(context: &mut dyn Context, arguments: Vec<Value>) -> NativeResult {
    let Some(value) = arguments.first() else {
        return Err(NativeError::argument_count_error(
            "serialize() expects exactly 1 argument, 0 given",
        ));
    };
    let mut encoder = Encoder::default();
    let mut value = value.try_deref()?.clone();
    loop {
        encoder.value(context, value).await?;
        let Some(next) = encoder.next_value() else {
            return Ok(Value::from(encoder.output.finish()));
        };
        value = next;
    }
}

impl Encoder {
    fn next_value(&mut self) -> Option<Value> {
        while let Some(frame) = self.frames.last_mut() {
            if let Some(position) = frame.position
                && let Some((key, value)) = frame.entries.entry_at(position)
            {
                frame.position = frame.entries.next_position(position);
                match key {
                    ArrayKey::Int(key) => self.output.integer(key),
                    ArrayKey::String(key) => self.output.string(key.as_bytes()),
                }
                let value = match value {
                    Value::Reference(reference) if Rc::strong_count(reference) == 1 => reference
                        .try_borrow()
                        .map_or_else(|_| value.clone(), |inner| inner.clone()),
                    _ => value.clone(),
                };
                if let Value::Array(array) = &value
                    && matches!(frame.kind, Contents::Array)
                    && Rc::ptr_eq(array, &frame.entries)
                {
                    return Some(Value::Null);
                }
                return Some(value);
            }
            self.frames.pop();
            self.output.close();
        }
        None
    }

    fn entries(&mut self, entries: Rc<PhpArray>, kind: Contents) {
        self.frames.push(Frame {
            position: entries.first_position(),
            entries,
            kind,
        });
    }

    fn seen(&mut self, value: &Value, resolved: &Value) -> bool {
        let is_reference = value.is_reference();
        let identity = match (resolved, value) {
            (Value::Object(object), _) => Identity::Object(Rc::as_ptr(object) as usize),
            (_, Value::Reference(reference)) => Identity::Reference(Rc::as_ptr(reference) as usize),
            _ => {
                self.slots += 1;
                return false;
            }
        };
        if let Some((slot, _)) = self.identities.get(&identity) {
            if *slot == 0 {
                self.slots += 1;
                self.output.null();
            } else if is_reference {
                self.output.reference(*slot);
            } else {
                self.slots += 1;
                self.output.object_reference(*slot);
            }
            true
        } else {
            self.slots += 1;
            let retained = if matches!(resolved, Value::Object(_)) {
                resolved
            } else {
                value
            };
            self.identities
                .insert(identity, (self.slots, retained.clone()));
            false
        }
    }

    async fn value(&mut self, context: &mut dyn Context, value: Value) -> Result<(), NativeError> {
        let resolved = value.try_deref()?.clone();
        if self.seen(&value, &resolved) {
            return Ok(());
        }
        match resolved {
            Value::Null => self.output.null(),
            Value::Bool(boolean) => self.output.boolean(boolean),
            Value::Int(integer) => self.output.integer(integer),
            Value::Float(float) => {
                let precision = context
                    .state()
                    .get::<SerializationSettings>()
                    .map_or(-1, |settings| settings.precision_digits);
                self.output
                    .float(&float_repr_with_precision(float, precision));
            }
            Value::String(string) => self.output.string(string.as_bytes()),
            Value::Array(array) => {
                self.output.array(array.len());
                self.entries(array, Contents::Array);
            }
            Value::Object(object) => self.object(context, object).await?,
            Value::Resource(_) => self.output.integer(0),
            Value::Reference(_) => return Err(NativeError::error("Invalid nested PHP reference")),
        }
        Ok(())
    }
}
