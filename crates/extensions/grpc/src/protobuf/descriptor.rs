//! Reads `google.protobuf.FileDescriptorSet`, what `protoc
//! --descriptor_set_out=set.pb --include_imports` writes, with the wire
//! reader itself: descriptor.proto needs no schema of its own here.
//!
//! The parts used, by field number:
//!
//! ```text
//! FileDescriptorSet    1 file*
//! FileDescriptorProto  2 package, 4 message_type*, 5 enum_type*, 6 service*,
//!                      8 options (50 features), 12 syntax, 14 edition
//! DescriptorProto      1 name, 2 field*, 3 nested_type*, 4 enum_type*,
//!                      7 options (7 map_entry, 12 features)
//! FieldDescriptorProto 1 name, 3 number, 4 label, 5 type, 6 type_name,
//!                      8 options (2 packed, 21 features), 9 oneof_index,
//!                      17 proto3_optional
//! EnumDescriptorProto  1 name
//! ServiceDescriptorProto 1 name, 2 method*
//! MethodDescriptorProto  1 name, 2 input_type, 3 output_type,
//!                        5 client_streaming, 6 server_streaming
//! FeatureSet           1 field_presence, 3 repeated_field_encoding,
//!                      4 utf8_validation
//! ```

use super::build::{FieldSpec, MessageSpec, MethodSpec, SchemaBuilder};
use super::wire::{Reader, WireError, WireType};
use super::{Kind, Schema};

pub fn parse(bytes: &[u8]) -> Result<Schema, String> {
    let mut builder = SchemaBuilder::default();
    let mut reader = Reader::new(bytes);
    while !reader.is_empty() {
        let (number, wire) = reader.tag().map_err(text)?;
        if number == 1 && wire == WireType::Len {
            file(reader.len_delimited().map_err(text)?, &mut builder).map_err(text)?;
        } else {
            reader.skip(wire).map_err(text)?;
        }
    }
    builder.finish()
}

fn text(error: impl ToString) -> String {
    format!("invalid FileDescriptorSet: {}", error.to_string())
}

/// The defaults that decide how a field is written: by syntax, edition and
/// `features` options, inherited from file to message to field.
#[derive(Clone, Copy)]
struct Features {
    implicit_presence: bool,
    packed: bool,
    utf8: bool,
}

impl Features {
    const PROTO2: Self = Self {
        implicit_presence: false,
        packed: false,
        utf8: false,
    };
    const PROTO3: Self = Self {
        implicit_presence: true,
        packed: true,
        utf8: true,
    };
    /// Edition 2023 and later.
    const EDITIONS: Self = Self {
        implicit_presence: false,
        packed: true,
        utf8: true,
    };

    /// Applies a `google.protobuf.FeatureSet`.
    fn apply(mut self, bytes: &[u8]) -> Result<Self, WireError> {
        let mut reader = Reader::new(bytes);
        while !reader.is_empty() {
            let (number, wire) = reader.tag()?;
            if wire != WireType::Varint {
                reader.skip(wire)?;
                continue;
            }
            let value = reader.varint()?;
            match number {
                // EXPLICIT = 1, IMPLICIT = 2, LEGACY_REQUIRED = 3
                1 => self.implicit_presence = value == 2,
                // PACKED = 1, EXPANDED = 2
                3 => self.packed = value == 1,
                // VERIFY = 2, NONE = 3
                4 => self.utf8 = value == 2,
                _ => {}
            }
        }
        Ok(self)
    }
}

/// The `features` (field `features_field`) inside an options message.
fn options_features(
    bytes: &[u8],
    features_field: u32,
    mut features: Features,
) -> Result<Features, WireError> {
    let mut reader = Reader::new(bytes);
    while !reader.is_empty() {
        let (number, wire) = reader.tag()?;
        if number == features_field && wire == WireType::Len {
            features = features.apply(reader.len_delimited()?)?;
        } else {
            reader.skip(wire)?;
        }
    }
    Ok(features)
}

fn string(bytes: &[u8]) -> Result<String, WireError> {
    // Names in descriptors are identifiers; anything else is not a schema.
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn file(bytes: &[u8], builder: &mut SchemaBuilder) -> Result<(), WireError> {
    let mut package = String::new();
    let mut syntax = String::new();
    let mut edition = 0u64;
    let mut options: &[u8] = &[];
    let mut messages: Vec<&[u8]> = Vec::new();
    let mut enums: Vec<&[u8]> = Vec::new();
    let mut services: Vec<&[u8]> = Vec::new();
    let mut reader = Reader::new(bytes);
    while !reader.is_empty() {
        let (number, wire) = reader.tag()?;
        match (number, wire) {
            (2, WireType::Len) => package = string(reader.len_delimited()?)?,
            (4, WireType::Len) => messages.push(reader.len_delimited()?),
            (5, WireType::Len) => enums.push(reader.len_delimited()?),
            (6, WireType::Len) => services.push(reader.len_delimited()?),
            (8, WireType::Len) => options = reader.len_delimited()?,
            (12, WireType::Len) => syntax = string(reader.len_delimited()?)?,
            (14, WireType::Varint) => edition = reader.varint()?,
            _ => reader.skip(wire)?,
        }
    }
    // Syntax is "proto2" when empty; editions set `edition` instead.
    let defaults = match syntax.as_str() {
        "proto3" => Features::PROTO3,
        "editions" => Features::EDITIONS,
        _ if edition != 0 => Features::EDITIONS,
        _ => Features::PROTO2,
    };
    let features = options_features(options, 50, defaults)?;
    for bytes in enums {
        builder.enums.push(qualified(&package, &enum_name(bytes)?));
    }
    for bytes in messages {
        message(bytes, &package, features, builder)?;
    }
    for bytes in services {
        service(bytes, &package, builder)?;
    }
    Ok(())
}

fn qualified(scope: &str, name: &str) -> String {
    if scope.is_empty() {
        name.to_owned()
    } else {
        format!("{scope}.{name}")
    }
}

fn enum_name(bytes: &[u8]) -> Result<String, WireError> {
    let mut reader = Reader::new(bytes);
    let mut name = String::new();
    while !reader.is_empty() {
        let (number, wire) = reader.tag()?;
        if number == 1 && wire == WireType::Len {
            name = string(reader.len_delimited()?)?;
        } else {
            reader.skip(wire)?;
        }
    }
    Ok(name)
}

fn message(
    bytes: &[u8],
    scope: &str,
    inherited: Features,
    builder: &mut SchemaBuilder,
) -> Result<(), WireError> {
    let mut name = String::new();
    let mut fields: Vec<&[u8]> = Vec::new();
    let mut nested: Vec<&[u8]> = Vec::new();
    let mut enums: Vec<&[u8]> = Vec::new();
    let mut options: &[u8] = &[];
    let mut reader = Reader::new(bytes);
    while !reader.is_empty() {
        let (number, wire) = reader.tag()?;
        match (number, wire) {
            (1, WireType::Len) => name = string(reader.len_delimited()?)?,
            (2, WireType::Len) => fields.push(reader.len_delimited()?),
            (3, WireType::Len) => nested.push(reader.len_delimited()?),
            (4, WireType::Len) => enums.push(reader.len_delimited()?),
            (7, WireType::Len) => options = reader.len_delimited()?,
            _ => reader.skip(wire)?,
        }
    }
    let full_name = qualified(scope, &name);
    let features = options_features(options, 12, inherited)?;
    let mut spec = MessageSpec {
        name: full_name.clone(),
        fields: Vec::with_capacity(fields.len()),
        map_entry: map_entry(options)?,
    };
    for bytes in fields {
        spec.fields.push(field(bytes, features)?);
    }
    builder.messages.push(spec);
    for bytes in enums {
        builder
            .enums
            .push(qualified(&full_name, &enum_name(bytes)?));
    }
    for bytes in nested {
        message(bytes, &full_name, features, builder)?;
    }
    Ok(())
}

/// `MessageOptions.map_entry`, field 7.
fn map_entry(options: &[u8]) -> Result<bool, WireError> {
    let mut reader = Reader::new(options);
    let mut entry = false;
    while !reader.is_empty() {
        let (number, wire) = reader.tag()?;
        if number == 7 && wire == WireType::Varint {
            entry = reader.varint()? != 0;
        } else {
            reader.skip(wire)?;
        }
    }
    Ok(entry)
}

fn field(bytes: &[u8], inherited: Features) -> Result<FieldSpec, WireError> {
    let mut spec = FieldSpec::default();
    let mut label = 1u64;
    let mut in_oneof = false;
    let mut proto3_optional = false;
    let mut options: &[u8] = &[];
    let mut reader = Reader::new(bytes);
    while !reader.is_empty() {
        let (number, wire) = reader.tag()?;
        match (number, wire) {
            (1, WireType::Len) => spec.name = string(reader.len_delimited()?)?,
            (3, WireType::Varint) => spec.number = reader.varint()? as u32,
            (4, WireType::Varint) => label = reader.varint()?,
            (5, WireType::Varint) => {
                let kind = reader.varint()?;
                if kind == 10 {
                    return Err(WireError::Group);
                }
                spec.kind = Kind::from_descriptor(kind);
            }
            (6, WireType::Len) => spec.type_name = Some(string(reader.len_delimited()?)?),
            (8, WireType::Len) => options = reader.len_delimited()?,
            (9, WireType::Varint) => {
                reader.varint()?;
                in_oneof = true;
            }
            (17, WireType::Varint) => proto3_optional = reader.varint()? != 0,
            _ => reader.skip(wire)?,
        }
    }
    let mut features = options_features(options, 21, inherited)?;
    // `[packed = true]` or `[packed = false]`, FieldOptions field 2.
    let mut options_reader = Reader::new(options);
    while !options_reader.is_empty() {
        let (number, wire) = options_reader.tag()?;
        if number == 2 && wire == WireType::Varint {
            features.packed = options_reader.varint()? != 0;
        } else {
            options_reader.skip(wire)?;
        }
    }
    // LABEL_REPEATED = 3; LABEL_REQUIRED = 2 also has presence.
    spec.repeated = label == 3;
    spec.packed = features.packed;
    spec.presence = !features.implicit_presence || in_oneof || proto3_optional || label == 2;
    spec.utf8 = features.utf8;
    // An enum field says TYPE_ENUM; its type name needs no resolving.
    if spec.kind == Some(Kind::Enum) {
        spec.type_name = None;
    }
    Ok(spec)
}

fn service(bytes: &[u8], package: &str, builder: &mut SchemaBuilder) -> Result<(), WireError> {
    let mut name = String::new();
    let mut methods: Vec<&[u8]> = Vec::new();
    let mut reader = Reader::new(bytes);
    while !reader.is_empty() {
        let (number, wire) = reader.tag()?;
        match (number, wire) {
            (1, WireType::Len) => name = string(reader.len_delimited()?)?,
            (2, WireType::Len) => methods.push(reader.len_delimited()?),
            _ => reader.skip(wire)?,
        }
    }
    let service = qualified(package, &name);
    for bytes in methods {
        let mut spec = MethodSpec {
            service: service.clone(),
            ..MethodSpec::default()
        };
        let mut reader = Reader::new(bytes);
        while !reader.is_empty() {
            let (number, wire) = reader.tag()?;
            match (number, wire) {
                (1, WireType::Len) => spec.name = string(reader.len_delimited()?)?,
                (2, WireType::Len) => spec.input = string(reader.len_delimited()?)?,
                (3, WireType::Len) => spec.output = string(reader.len_delimited()?)?,
                (5, WireType::Varint) => spec.client_streaming = reader.varint()? != 0,
                (6, WireType::Varint) => spec.server_streaming = reader.varint()? != 0,
                _ => reader.skip(wire)?,
            }
        }
        builder.methods.push(spec);
    }
    Ok(())
}
