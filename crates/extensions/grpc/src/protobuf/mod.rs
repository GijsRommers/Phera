//! Protocol Buffers messages as PHP arrays, over a schema kept in flat tables.
//!
//! ```php
//! $schema = Phera\Protobuf\Schema::define([
//!     'echo.Req' => [1 => ['name' => 'message', 'type' => 'string']],
//! ]);
//! $bytes = $schema->encode('echo.Req', ['message' => 'hi']); // "\x0a\x02hi"
//! $schema->decode('echo.Req', $bytes);                       // ['message' => 'hi']
//! ```
//!
//! # Layout
//!
//! A [`Schema`] is two vectors and two sorted indexes, built once:
//!
//! ```text
//! messages: [MessageDef]   name, fields first..first+count, direct[64]
//! fields:   [FieldDef]     every message's fields, contiguous, by number
//! by_name:  [u32]          message ids sorted by name (binary search)
//! methods:  [MethodDef]    service methods sorted by "/pkg.Service/Method"
//! ```
//!
//! A message refers to another by its index in `messages`, never by name or
//! pointer. Decoding finds a field by number through the message's `direct`
//! table for numbers 1 to 64 (nearly every field ever written), and by
//! binary search over its sorted field slice above that: no hash map is
//! consulted per field on the wire. A field's PHP array key is made once,
//! when the schema is built, and shared by every decoded array.

mod build;
mod decode;
mod define;
mod descriptor;
mod encode;
pub(crate) mod php;
pub mod wire;

#[cfg(test)]
mod tests;

pub use build::{FieldSpec, MessageSpec, MethodSpec, SchemaBuilder};
pub use decode::{DecodeError, decode};
pub use encode::{EncodeError, encode};

use phera_extension_api::ArrayKey;

/// Field types, numbered as in `google.protobuf.FieldDescriptorProto.Type`
/// (10, groups, is not supported).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Double = 1,
    Float = 2,
    Int64 = 3,
    Uint64 = 4,
    Int32 = 5,
    Fixed64 = 6,
    Fixed32 = 7,
    Bool = 8,
    String = 9,
    Message = 11,
    Bytes = 12,
    Uint32 = 13,
    Enum = 14,
    Sfixed32 = 15,
    Sfixed64 = 16,
    Sint32 = 17,
    Sint64 = 18,
}

impl Kind {
    pub fn from_descriptor(number: u64) -> Option<Self> {
        Some(match number {
            1 => Self::Double,
            2 => Self::Float,
            3 => Self::Int64,
            4 => Self::Uint64,
            5 => Self::Int32,
            6 => Self::Fixed64,
            7 => Self::Fixed32,
            8 => Self::Bool,
            9 => Self::String,
            11 => Self::Message,
            12 => Self::Bytes,
            13 => Self::Uint32,
            14 => Self::Enum,
            15 => Self::Sfixed32,
            16 => Self::Sfixed64,
            17 => Self::Sint32,
            18 => Self::Sint64,
            _ => return None,
        })
    }

    /// The scalar type names of a `.proto` file.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "double" => Self::Double,
            "float" => Self::Float,
            "int64" => Self::Int64,
            "uint64" => Self::Uint64,
            "int32" => Self::Int32,
            "fixed64" => Self::Fixed64,
            "fixed32" => Self::Fixed32,
            "bool" => Self::Bool,
            "string" => Self::String,
            "bytes" => Self::Bytes,
            "uint32" => Self::Uint32,
            "enum" => Self::Enum,
            "sfixed32" => Self::Sfixed32,
            "sfixed64" => Self::Sfixed64,
            "sint32" => Self::Sint32,
            "sint64" => Self::Sint64,
            "message" => Self::Message,
            _ => return None,
        })
    }

    pub fn wire_type(self) -> wire::WireType {
        use wire::WireType;
        match self {
            Self::Double | Self::Fixed64 | Self::Sfixed64 => WireType::Fixed64,
            Self::Float | Self::Fixed32 | Self::Sfixed32 => WireType::Fixed32,
            Self::String | Self::Bytes | Self::Message => WireType::Len,
            _ => WireType::Varint,
        }
    }

    /// Numbers, booleans and enums can be packed into one byte run.
    pub fn is_packable(self) -> bool {
        !matches!(self, Self::String | Self::Bytes | Self::Message)
    }

    /// Types that may be the key of a map field.
    pub fn is_map_key(self) -> bool {
        !matches!(
            self,
            Self::Double | Self::Float | Self::Bytes | Self::Message | Self::Enum
        )
    }
}

/// Bits of [`FieldDef::flags`].
pub mod flags {
    pub const REPEATED: u8 = 1;
    /// Repeated numbers are written as one length-delimited run.
    pub const PACKED: u8 = 2;
    /// A `map<K, V>`: repeated entry messages, keys 1 and values 2.
    pub const MAP: u8 = 4;
    /// The field is written whenever it is set, even to its default, and a
    /// decoded array leaves it out when the message did not carry it
    /// (message fields, oneof members, `optional`).
    pub const PRESENCE: u8 = 8;
    /// Strings must be valid UTF-8 (proto3 and editions).
    pub const UTF8: u8 = 16;
}

/// One field of one message. 32 bytes; a message's fields are adjacent.
#[derive(Debug)]
pub struct FieldDef {
    pub number: u32,
    /// For a message field the message's index; for a map field the index
    /// of its entry message.
    pub message: u32,
    pub kind: Kind,
    pub flags: u8,
    /// The PHP array key, made once.
    pub key: ArrayKey,
}

impl FieldDef {
    #[inline]
    pub fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    pub fn name(&self) -> String {
        match &self.key {
            ArrayKey::Int(number) => number.to_string(),
            ArrayKey::Interned(name) => name.to_string_lossy(),
            ArrayKey::String(name) => name.to_string_lossy(),
        }
    }
}

/// Field numbers below this are found with one table read.
const DIRECT: usize = 64;

#[derive(Debug)]
pub struct MessageDef {
    /// The full name, without a leading dot: `echo.Req`.
    pub name: Box<str>,
    first_field: u32,
    field_count: u32,
    /// `direct[n - 1]` is one more than the local index of field `n`, or 0.
    direct: [u16; DIRECT],
    pub map_entry: bool,
}

#[derive(Debug)]
pub struct MethodDef {
    /// `/pkg.Service/Method`, the HTTP/2 path of a call.
    pub path: Box<str>,
    pub input: u32,
    pub output: u32,
    pub client_streaming: bool,
    pub server_streaming: bool,
}

#[derive(Debug, Default)]
pub struct Schema {
    messages: Vec<MessageDef>,
    fields: Vec<FieldDef>,
    by_name: Vec<u32>,
    methods: Vec<MethodDef>,
}

impl Schema {
    /// The index of a message by full name; a leading dot is allowed.
    pub fn message_id(&self, name: &str) -> Option<u32> {
        let name = name.strip_prefix('.').unwrap_or(name);
        self.by_name
            .binary_search_by(|&id| (*self.messages[id as usize].name).cmp(name))
            .ok()
            .map(|position| self.by_name[position])
    }

    #[inline]
    pub fn message(&self, id: u32) -> &MessageDef {
        &self.messages[id as usize]
    }

    #[inline]
    pub fn fields(&self, id: u32) -> &[FieldDef] {
        let message = self.message(id);
        let first = message.first_field as usize;
        &self.fields[first..first + message.field_count as usize]
    }

    /// The local index (into [`Schema::fields`]) of field `number`.
    #[inline]
    pub fn local_index(&self, id: u32, number: u32) -> Option<usize> {
        let message = self.message(id);
        if let Some(&slot) = message.direct.get((number as usize).wrapping_sub(1)) {
            return (slot as usize).checked_sub(1);
        }
        self.fields(id)
            .binary_search_by_key(&number, |field| field.number)
            .ok()
    }

    pub fn method(&self, path: &str) -> Option<&MethodDef> {
        self.methods
            .binary_search_by(|method| (*method.path).cmp(path))
            .ok()
            .map(|position| &self.methods[position])
    }

    /// Message names in name order, map entries left out.
    pub fn message_names(&self) -> impl Iterator<Item = &str> {
        self.by_name
            .iter()
            .map(|&id| &self.messages[id as usize])
            .filter(|message| !message.map_entry)
            .map(|message| &*message.name)
    }

    pub fn methods(&self) -> &[MethodDef] {
        &self.methods
    }
}
