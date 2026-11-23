//! PHP supplies the declared default for a hole in a native argument list.
//!
//! ```php
//! json_decode($json, flags: JSON_THROW_ON_ERROR); // null, then 512
//! array_keys($array, strict: true); // the default of $filter_value is unknown
//! ```
//!
//! The catalog is generated from php-src. Positional calls use only the
//! extension's argument parser. Named gaps and reflection also read this data.

use std::collections::HashMap;
use std::sync::OnceLock;

use phera_value::{PhpArray, Value};

const STUBS: &str = include_str!("../data/native-parameters.tsv");

#[derive(Clone, Copy)]
pub struct Signature {
    pub source: &'static str,
    fields: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StubParameter {
    pub name: &'static str,
    pub optional: bool,
    pub variadic: bool,
    pub by_ref: bool,
    pub expression: &'static str,
    pub value: &'static str,
}

#[derive(Clone, Copy)]
pub enum StubDefault {
    Null,
    Bool(bool),
    EmptyArray,
    Int(i64),
    Float(f64),
    Bytes(&'static str),
    EnumCase(&'static str, &'static str),
}

pub fn signature(name: &str) -> Option<Signature> {
    static SIGNATURES: OnceLock<HashMap<&str, Signature>> = OnceLock::new();
    let signatures = SIGNATURES.get_or_init(|| {
        STUBS
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| {
                let (name, record) = line.split_once('\t')?;
                let (source, fields) = record.split_once('\t').unwrap_or((record, ""));
                Some((name, Signature { source, fields }))
            })
            .collect()
    });
    if name.bytes().any(|byte| byte.is_ascii_uppercase()) {
        signatures.get(name.to_ascii_lowercase().as_str()).copied()
    } else {
        signatures.get(name).copied()
    }
}

impl Signature {
    pub fn parameters(self) -> impl Iterator<Item = StubParameter> {
        let mut fields = self.fields.split('\t');
        std::iter::from_fn(move || {
            let name = fields.next()?;
            let flags = fields.next()?;
            Some(StubParameter {
                name,
                optional: flags.starts_with('o'),
                variadic: flags.as_bytes().get(1) == Some(&b'v'),
                by_ref: flags.ends_with('&'),
                expression: fields.next()?,
                value: fields.next()?,
            })
        })
    }
}

impl StubParameter {
    pub fn default(self) -> Option<StubDefault> {
        if !self.optional {
            return None;
        }
        let (kind, payload) = self.value.split_at_checked(1)?;
        match kind {
            "n" => Some(StubDefault::Null),
            "t" => Some(StubDefault::Bool(true)),
            "f" => Some(StubDefault::Bool(false)),
            "a" => Some(StubDefault::EmptyArray),
            "i" => payload.parse().ok().map(StubDefault::Int),
            "d" => payload.parse().ok().map(StubDefault::Float),
            "s" => Some(StubDefault::Bytes(payload)),
            "e" => {
                let (class, case) = payload.split_once("::")?;
                Some(StubDefault::EnumCase(class, case))
            }
            _ => None,
        }
    }

    pub fn constant_name(self) -> Option<&'static str> {
        if let Some(name) = self.value.strip_prefix('e') {
            return Some(name);
        }
        let text = self.expression;
        let (class, constant) = text.split_once("::").unwrap_or(("", text));
        fn identifier(text: &str) -> bool {
            !text.is_empty()
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'\\'))
                && text.as_bytes()[0].is_ascii_alphabetic()
        }
        (self.default().is_some()
            && identifier(constant)
            && !matches!(constant, "null" | "true" | "false" | "class")
            && (class.is_empty() || identifier(class)))
        .then_some(text)
    }
}

impl StubDefault {
    pub fn literal(self) -> Option<Value> {
        match self {
            Self::Null => Some(Value::Null),
            Self::Bool(value) => Some(Value::Bool(value)),
            Self::EmptyArray => Some(Value::from(PhpArray::new())),
            Self::Int(number) => Some(Value::Int(number)),
            Self::Float(number) => Some(Value::Float(number)),
            Self::Bytes(hex) => {
                let bytes: Option<Vec<_>> = hex
                    .as_bytes()
                    .chunks(2)
                    .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
                    .collect();
                bytes.map(Value::from)
            }
            Self::EnumCase(..) => None,
        }
    }
}
