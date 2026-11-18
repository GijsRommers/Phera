//! `serialize([$object, $object])` emits `r:2;` for the second object occurrence.

mod objects;

use std::{collections::HashMap, rc::Rc};

use phera_extension_api::serialization::precision;
use phera_extension_api::value::with_float_repr;
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, Object, PhpArray, Value};

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
    slots: SerializationSlots,
    precision: Option<i32>,
}

/// Serializable methods may call serialize() with the parent's slot table.
#[derive(Default)]
struct SerializationSlots {
    identities: HashMap<Identity, (usize, Value)>,
    count: usize,
}

pub async fn serialize(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let Some(value) = arguments.first() else {
        return Err(NativeError::argument_count_error(
            "serialize() expects exactly 1 argument, 0 given",
        ));
    };
    let inherited = context.state().take::<SerializationSlots>();
    let restore = inherited.is_some();
    let mut encoder = Encoder {
        slots: inherited.unwrap_or_default(),
        ..Encoder::default()
    };
    let result = encoder.write_value(context, value).await;
    if restore {
        *context.state().get_or_default::<SerializationSlots>() =
            std::mem::take(&mut encoder.slots);
    }
    result?;
    Ok(Value::from(encoder.output.finish()))
}

impl Encoder {
    async fn write_value(
        &mut self,
        context: &mut dyn Context,
        value: &Value,
    ) -> Result<(), NativeError> {
        let mut value = value.try_deref()?.clone();
        loop {
            if let Some(object) = self.value(context, value)?
                && !self.write_plain_object(context, &object)?
            {
                self.object(context, object).await?;
                // A hook can change the request's serialization precision.
                self.precision = None;
            }
            let Some(next) = self.next_value(context) else {
                return Ok(());
            };
            value = next;
        }
    }

    fn next_value(&mut self, context: &mut dyn Context) -> Option<Value> {
        while let Some(frame) = self.frames.last_mut() {
            if let Some(position) = frame.position
                && let Some((key, value)) = frame.entries.entry_at(position)
            {
                frame.position = frame.entries.next_position(position);
                match key {
                    ArrayKey::Int(key) => self.output.integer(key),
                    ArrayKey::String(key) => self.output.string(key.as_bytes()),
                }
                if write_scalar(&mut self.output, &mut self.precision, context, value) {
                    self.slots.count += 1;
                    continue;
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
                self.slots.count += 1;
                return false;
            }
        };
        if let Some((slot, _)) = self.slots.identities.get(&identity) {
            if *slot == 0 {
                self.slots.count += 1;
                self.output.null();
            } else if is_reference {
                self.output.reference(*slot);
            } else {
                self.slots.count += 1;
                self.output.object_reference(*slot);
            }
            true
        } else {
            self.slots.count += 1;
            let retained = if matches!(resolved, Value::Object(_)) {
                resolved
            } else {
                value
            };
            self.slots
                .identities
                .insert(identity, (self.slots.count, retained.clone()));
            false
        }
    }

    fn value(
        &mut self,
        context: &mut dyn Context,
        value: Value,
    ) -> Result<Option<Rc<Object>>, NativeError> {
        let resolved = if matches!(value, Value::Reference(_) | Value::Object(_)) {
            let resolved = value.try_deref()?.clone();
            if self.seen(&value, &resolved) {
                return Ok(None);
            }
            resolved
        } else {
            self.slots.count += 1;
            value
        };
        match resolved {
            Value::Array(array) => {
                self.output.array(array.len());
                self.entries(array, Contents::Array);
            }
            Value::Object(object) => return Ok(Some(object)),
            scalar => {
                if !write_scalar(&mut self.output, &mut self.precision, context, &scalar) {
                    return Err(NativeError::error("Invalid nested PHP reference"));
                }
            }
        }
        Ok(None)
    }
}

fn write_scalar(
    output: &mut Writer,
    cached_precision: &mut Option<i32>,
    context: &mut dyn Context,
    value: &Value,
) -> bool {
    match value {
        Value::Null => output.null(),
        Value::Bool(boolean) => output.boolean(*boolean),
        Value::Int(integer) => output.integer(*integer),
        Value::Float(float) => {
            let precision = *cached_precision.get_or_insert_with(|| precision(context));
            if precision >= 0 || !output.short_float(*float) {
                with_float_repr(*float, precision, |decimal| output.float(decimal));
            }
        }
        Value::String(string) => output.string(string.as_bytes()),
        Value::Resource(_) => output.integer(0),
        _ => return false,
    }
    true
}
