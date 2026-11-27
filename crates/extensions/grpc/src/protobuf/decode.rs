//! Protobuf bytes to PHP array.
//!
//! While a message is read, each of its fields has a slot in a table indexed
//! by the field's local index (its position in the message's field slice).
//! A repeated field collects its elements in its slot, so no PHP array is
//! looked up or grown per element; the PHP array is built at the end, in
//! field-number order, with each key written once. The slot tables are
//! kept per nesting depth and reused for every message at that depth.
//!
//! What a decoded array holds:
//! - every field without presence, at its default when absent (`0`, `''`,
//!   `false`, `0.0`), and every repeated or map field (`[]` when absent);
//! - a field with presence (a message, a oneof member, `optional`) only when
//!   the bytes carried it;
//! - 64-bit unsigned values above `PHP_INT_MAX` as their two's complement
//!   (negative) int, as Google's PHP library does on 64-bit builds.
//!
//! Unknown fields are skipped, and so is a known field whose wire type does
//! not match its declaration, as the C++ parser does.

use std::fmt;

use phera_extension_api::{ArrayKey, PhpArray, Value};

use super::wire::{Reader, WireError, WireType, unzigzag32, unzigzag64};
use super::{FieldDef, Kind, Schema, flags};

const MAX_DEPTH: usize = 100;

#[derive(Debug)]
pub struct DecodeError {
    message: String,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<WireError> for DecodeError {
    #[cold]
    fn from(error: WireError) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

impl DecodeError {
    #[cold]
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub fn decode(schema: &Schema, id: u32, bytes: &[u8]) -> Result<PhpArray, DecodeError> {
    Decoder {
        schema,
        slot_tables: Vec::new(),
    }
    .message(id, bytes, 0)
    .map_err(|error| DecodeError::new(format!("{}: {error}", schema.message(id).name)))
}

/// What a field has collected so far. `'b` is the input's lifetime: a
/// singular message field keeps its bytes until the whole message is read,
/// because protobuf merges a message field that occurs more than once, and
/// merging the bytes (concatenating them) is exact where merging decoded
/// arrays is not (defaults would overwrite values).
#[derive(Default)]
enum Slot<'b> {
    #[default]
    Empty,
    One(Value),
    Many(Vec<Value>),
    Map(PhpArray),
    Message(&'b [u8]),
    Merged(Vec<u8>),
}

struct Decoder<'s, 'b> {
    schema: &'s Schema,
    /// One reusable slot table per nesting depth.
    slot_tables: Vec<Vec<Slot<'b>>>,
}

impl<'b> Decoder<'_, 'b> {
    fn message(&mut self, id: u32, bytes: &'b [u8], depth: usize) -> Result<PhpArray, DecodeError> {
        if depth > MAX_DEPTH {
            return Err(DecodeError::new("messages are nested too deeply"));
        }
        let schema = self.schema;
        let fields = schema.fields(id);
        let mut slots = self.take_slots(depth, fields.len());
        let result = self
            .read_fields(id, fields, bytes, depth, &mut slots)
            .and_then(|nested| match nested {
                true => self.decode_nested(fields, depth, &mut slots),
                false => Ok(()),
            });
        let array = result.map(|()| assemble(fields, &mut slots));
        self.slot_tables[depth] = slots;
        array
    }

    /// Decodes the singular message fields once their bytes are complete.
    fn decode_nested(
        &mut self,
        fields: &[FieldDef],
        depth: usize,
        slots: &mut [Slot<'b>],
    ) -> Result<(), DecodeError> {
        for (field, slot) in fields.iter().zip(slots.iter_mut()) {
            let nested = match std::mem::take(slot) {
                Slot::Message(bytes) => self.message(field.message, bytes, depth + 1),
                // Rare: a fresh decoder, as the joined bytes are local.
                Slot::Merged(bytes) => Decoder {
                    schema: self.schema,
                    slot_tables: Vec::new(),
                }
                .message(field.message, &bytes, depth + 1),
                other => {
                    *slot = other;
                    continue;
                }
            };
            let nested =
                nested.map_err(|error| DecodeError::new(format!("{}: {error}", field.name())))?;
            *slot = Slot::One(Value::from(nested));
        }
        Ok(())
    }

    /// Reads the fields into their slots; true when a singular message
    /// field still has to be decoded.
    fn read_fields(
        &mut self,
        id: u32,
        fields: &[FieldDef],
        bytes: &'b [u8],
        depth: usize,
        slots: &mut [Slot<'b>],
    ) -> Result<bool, DecodeError> {
        let mut nested = false;
        let mut reader = Reader::new(bytes);
        while !reader.is_empty() {
            let (number, wire) = reader.tag()?;
            let Some(local) = self.schema.local_index(id, number) else {
                reader.skip(wire)?;
                continue;
            };
            let field = &fields[local];
            let slot = &mut slots[local];
            if field.has(flags::MAP) {
                if wire != WireType::Len {
                    reader.skip(wire)?;
                    continue;
                }
                let entry = reader.len_delimited()?;
                let (key, value) = self.map_entry(field.message, entry, depth + 1)?;
                if !matches!(slot, Slot::Map(_)) {
                    *slot = Slot::Map(PhpArray::new());
                }
                if let Slot::Map(map) = slot {
                    map.insert(key, value);
                }
            } else if field.has(flags::REPEATED) {
                if !matches!(slot, Slot::Many(_)) {
                    *slot = Slot::Many(Vec::new());
                }
                let Slot::Many(items) = slot else {
                    unreachable!("the slot was just made a list");
                };
                // Either encoding is accepted, whatever the declaration says.
                if wire == WireType::Len && field.kind.is_packable() {
                    let mut packed = Reader::new(reader.len_delimited()?);
                    let wire = field.kind.wire_type();
                    while !packed.is_empty() {
                        items.push(scalar(field, wire, &mut packed)?);
                    }
                } else if wire == field.kind.wire_type() {
                    let item = self.value(field, wire, &mut reader, depth)?;
                    items.push(item);
                } else {
                    reader.skip(wire)?;
                }
            } else if wire == field.kind.wire_type() {
                if field.kind == Kind::Message {
                    let bytes = reader.len_delimited()?;
                    nested = true;
                    *slot = match std::mem::take(slot) {
                        Slot::Message(first) => Slot::Merged([first, bytes].concat()),
                        Slot::Merged(mut joined) => {
                            joined.extend_from_slice(bytes);
                            Slot::Merged(joined)
                        }
                        _ => Slot::Message(bytes),
                    };
                } else {
                    *slot = Slot::One(scalar(field, wire, &mut reader)?);
                }
            } else {
                reader.skip(wire)?;
            }
        }
        Ok(nested)
    }

    #[inline]
    fn value(
        &mut self,
        field: &FieldDef,
        wire: WireType,
        reader: &mut Reader<'b>,
        depth: usize,
    ) -> Result<Value, DecodeError> {
        if field.kind == Kind::Message {
            let bytes = reader.len_delimited()?;
            let nested = self
                .message(field.message, bytes, depth + 1)
                .map_err(|error| DecodeError::new(format!("{}: {error}", field.name())))?;
            return Ok(Value::from(nested));
        }
        scalar(field, wire, reader)
    }

    fn map_entry(
        &mut self,
        entry: u32,
        bytes: &'b [u8],
        depth: usize,
    ) -> Result<(ArrayKey, Value), DecodeError> {
        let fields = self.schema.fields(entry);
        let (key_field, value_field) = (&fields[0], &fields[1]);
        let mut key = None;
        let mut value = None;
        let mut reader = Reader::new(bytes);
        while !reader.is_empty() {
            let (number, wire) = reader.tag()?;
            let field = match number {
                1 => key_field,
                2 => value_field,
                _ => {
                    reader.skip(wire)?;
                    continue;
                }
            };
            if wire != field.kind.wire_type() {
                reader.skip(wire)?;
                continue;
            }
            let decoded = self.value(field, wire, &mut reader, depth)?;
            if number == 1 {
                key = Some(decoded);
            } else {
                value = Some(decoded);
            }
        }
        let key = key.unwrap_or_else(|| default(key_field));
        let key = match key {
            Value::Bool(flag) => ArrayKey::Int(i64::from(flag)),
            Value::Int(number) => ArrayKey::Int(number),
            other => {
                ArrayKey::from_value(&other).map_err(|error| DecodeError::new(error.message))?
            }
        };
        let value = value.unwrap_or_else(|| match value_field.kind {
            Kind::Message => Value::from(PhpArray::new()),
            _ => default(value_field),
        });
        Ok((key, value))
    }

    fn take_slots(&mut self, depth: usize, count: usize) -> Vec<Slot<'b>> {
        while self.slot_tables.len() <= depth {
            self.slot_tables.push(Vec::new());
        }
        let mut slots = std::mem::take(&mut self.slot_tables[depth]);
        slots.clear();
        slots.resize_with(count, Slot::default);
        slots
    }
}

/// The PHP array of one message, its keys in field-number order.
fn assemble(fields: &[FieldDef], slots: &mut [Slot<'_>]) -> PhpArray {
    PhpArray::build_distinct(fields.len(), |entries| {
        for (field, slot) in fields.iter().zip(slots.iter_mut()) {
            let value = match std::mem::take(slot) {
                Slot::One(value) => value,
                Slot::Many(items) => {
                    let len = items.len();
                    Value::from(PhpArray::from_values(items, len))
                }
                Slot::Map(map) => Value::from(map),
                // Decoded by `decode_nested` before this runs.
                Slot::Message(_) | Slot::Merged(_) => continue,
                Slot::Empty if field.has(flags::REPEATED) => Value::from(PhpArray::new()),
                Slot::Empty if field.has(flags::PRESENCE) => continue,
                Slot::Empty => default(field),
            };
            entries.push(field.key.clone(), value);
        }
    })
}

fn default(field: &FieldDef) -> Value {
    match field.kind {
        Kind::String | Kind::Bytes => Value::interned(b""),
        Kind::Bool => Value::Bool(false),
        Kind::Double | Kind::Float => Value::Float(0.0),
        Kind::Message => Value::Null,
        _ => Value::Int(0),
    }
}

#[inline]
fn scalar(field: &FieldDef, wire: WireType, reader: &mut Reader<'_>) -> Result<Value, DecodeError> {
    Ok(match wire {
        WireType::Varint => {
            let raw = reader.varint()?;
            match field.kind {
                Kind::Bool => Value::Bool(raw != 0),
                // A 32-bit value keeps only its low 32 bits.
                Kind::Int32 | Kind::Enum => Value::Int(i64::from(raw as i32)),
                Kind::Uint32 => Value::Int(i64::from(raw as u32)),
                Kind::Sint32 => Value::Int(i64::from(unzigzag32(raw as u32))),
                Kind::Sint64 => Value::Int(unzigzag64(raw)),
                _ => Value::Int(raw as i64),
            }
        }
        WireType::Fixed32 => {
            let raw = reader.fixed32()?;
            match field.kind {
                Kind::Float => Value::Float(f64::from(f32::from_bits(raw))),
                Kind::Sfixed32 => Value::Int(i64::from(raw as i32)),
                _ => Value::Int(i64::from(raw)),
            }
        }
        WireType::Fixed64 => {
            let raw = reader.fixed64()?;
            match field.kind {
                Kind::Double => Value::Float(f64::from_bits(raw)),
                _ => Value::Int(raw as i64),
            }
        }
        WireType::Len => {
            let bytes = reader.len_delimited()?;
            if field.has(flags::UTF8) && std::str::from_utf8(bytes).is_err() {
                return Err(DecodeError::new(format!(
                    "{}: string is not valid UTF-8",
                    field.name()
                )));
            }
            Value::from(bytes)
        }
        WireType::StartGroup | WireType::EndGroup => return Err(WireError::Group.into()),
    })
}
