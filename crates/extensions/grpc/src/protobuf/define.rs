//! A schema from PHP arrays, for prototypes and tests without `protoc`.
//!
//! ```php
//! Schema::define([
//!     'shop.Item' => [
//!         1 => ['name' => 'id', 'type' => 'int64'],
//!         2 => ['name' => 'tags', 'type' => 'string', 'repeated' => true],
//!         3 => ['name' => 'price', 'type' => 'shop.Money'],          // a message
//!         4 => ['name' => 'stock', 'type' => 'map', 'key' => 'string', 'value' => 'int32'],
//!         5 => ['name' => 'note', 'type' => 'string', 'optional' => true],
//!     ],
//!     'shop.Money' => [1 => ['name' => 'cents', 'type' => 'int64']],
//! ], [
//!     'shop.Shop' => [
//!         'Get'   => ['shop.Item', 'shop.Item'],
//!         'Watch' => ['input' => 'shop.Item', 'output' => 'shop.Item', 'serverStreaming' => true],
//!     ],
//! ]);
//! ```
//!
//! Fields follow proto3 rules: no presence unless `optional`, repeated
//! numbers packed unless `'packed' => false`, strings checked for UTF-8.

use phera_extension_api::{ArrayKey, PhpArray, Value};

use super::build::{FieldSpec, MessageSpec, MethodSpec, SchemaBuilder};
use super::{Kind, Schema};

pub fn parse(messages: &PhpArray, services: &PhpArray) -> Result<Schema, String> {
    let mut builder = SchemaBuilder::default();
    for (name, fields) in messages.iter() {
        let name = key_text(&name);
        let name = name.strip_prefix('.').unwrap_or(&name).to_owned();
        let Value::Array(fields) = &*fields.deref() else {
            return Err(format!("message {name} must be an array of fields"));
        };
        let mut spec = MessageSpec {
            name: name.clone(),
            fields: Vec::with_capacity(fields.len()),
            map_entry: false,
        };
        for (number, field) in fields.iter() {
            let ArrayKey::Int(number) = number else {
                return Err(format!(
                    "message {name}: field key \"{}\" is not a field number",
                    key_text(&number)
                ));
            };
            let Value::Array(field) = &*field.deref() else {
                return Err(format!("message {name}: field {number} must be an array"));
            };
            let number = u32::try_from(number)
                .map_err(|_| format!("message {name}: invalid field number {number}"))?;
            spec.fields
                .push(field_spec(&name, number, field, &mut builder)?);
        }
        builder.messages.push(spec);
    }
    for (service, methods) in services.iter() {
        let service = key_text(&service);
        let Value::Array(methods) = &*methods.deref() else {
            return Err(format!("service {service} must be an array of methods"));
        };
        for (method, types) in methods.iter() {
            builder
                .methods
                .push(method_spec(&service, &key_text(&method), &types.deref())?);
        }
    }
    builder.finish()
}

fn field_spec(
    message: &str,
    number: u32,
    field: &PhpArray,
    builder: &mut SchemaBuilder,
) -> Result<FieldSpec, String> {
    let at = || format!("message {message}, field {number}");
    let name = text(field, "name").ok_or_else(|| format!("{}: \"name\" must be a string", at()))?;
    let type_name =
        text(field, "type").ok_or_else(|| format!("{}: \"type\" must be a string", at()))?;
    let mut spec = FieldSpec {
        name: name.clone(),
        number,
        repeated: flag(field, "repeated")?,
        packed: field
            .get(&ArrayKey::str("packed"))
            .is_none_or(phera_extension_api::value::to_bool),
        presence: flag(field, "optional")?,
        utf8: true,
        ..FieldSpec::default()
    };
    if type_name == "map" {
        let key = text(field, "key").ok_or_else(|| format!("{}: a map needs \"key\"", at()))?;
        let value =
            text(field, "value").ok_or_else(|| format!("{}: a map needs \"value\"", at()))?;
        let entry = format!("{message}.{}Entry", camel(&name));
        builder.messages.push(MessageSpec {
            name: entry.clone(),
            fields: vec![typed("key", 1, &key), typed("value", 2, &value)],
            map_entry: true,
        });
        spec.kind = Some(Kind::Message);
        spec.type_name = Some(format!(".{entry}"));
        spec.repeated = true;
        return Ok(spec);
    }
    match Kind::from_name(&type_name) {
        Some(Kind::Message) => {
            spec.kind = Some(Kind::Message);
            spec.type_name = Some(
                text(field, "message")
                    .ok_or_else(|| format!("{}: a message field needs \"message\"", at()))?,
            );
        }
        Some(kind) => spec.kind = Some(kind),
        // Any other name is a message type, resolved like a .proto name.
        None => spec.type_name = Some(type_name),
    }
    Ok(spec)
}

/// A map entry's key or value field.
fn typed(name: &str, number: u32, type_name: &str) -> FieldSpec {
    let kind = Kind::from_name(type_name);
    FieldSpec {
        name: name.to_owned(),
        number,
        kind: kind.filter(|kind| *kind != Kind::Message),
        type_name: kind.is_none().then(|| type_name.to_owned()),
        utf8: true,
        ..FieldSpec::default()
    }
}

fn method_spec(service: &str, name: &str, types: &Value) -> Result<MethodSpec, String> {
    let at = || format!("service {service}, method {name}");
    let Value::Array(types) = types else {
        return Err(format!(
            "{}: must be [input, output] or an array of options",
            at()
        ));
    };
    let input = text(types, "input").or_else(|| list_text(types, 0));
    let output = text(types, "output").or_else(|| list_text(types, 1));
    let (Some(input), Some(output)) = (input, output) else {
        return Err(format!(
            "{}: needs an input and an output message type",
            at()
        ));
    };
    Ok(MethodSpec {
        service: service.strip_prefix('.').unwrap_or(service).to_owned(),
        name: name.to_owned(),
        input,
        output,
        client_streaming: flag(types, "clientStreaming")?,
        server_streaming: flag(types, "serverStreaming")?,
    })
}

/// `Item` from `item`, `ItemPrice` from `item_price`, as protoc names map entries.
fn camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = true;
    for character in name.chars() {
        if character == '_' {
            upper = true;
        } else if upper {
            out.extend(character.to_uppercase());
            upper = false;
        } else {
            out.push(character);
        }
    }
    out
}

fn key_text(key: &ArrayKey) -> String {
    match key {
        ArrayKey::Int(number) => number.to_string(),
        ArrayKey::Interned(name) => name.to_string_lossy(),
        ArrayKey::String(name) => name.to_string_lossy(),
    }
}

fn text(array: &PhpArray, key: &str) -> Option<String> {
    let value = array.get(&ArrayKey::str(key))?;
    let bytes = value.deref().as_str_bytes()?.to_vec();
    String::from_utf8(bytes).ok()
}

fn list_text(array: &PhpArray, index: i64) -> Option<String> {
    let value = array.get_int(index)?;
    let bytes = value.deref().as_str_bytes()?.to_vec();
    String::from_utf8(bytes).ok()
}

fn flag(array: &PhpArray, key: &str) -> Result<bool, String> {
    match array
        .get(&ArrayKey::str(key))
        .map(|value| value.deref().clone())
    {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(flag)) => Ok(flag),
        Some(other) => Err(format!(
            "\"{key}\" must be a boolean, got {}",
            other.type_name()
        )),
    }
}
