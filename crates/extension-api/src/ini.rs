//! A setting belongs to a loaded PHP module and keeps its original spelling.
//!
//! ```php
//! ini_get('memory_limit'); // "128M"
//! ini_get('opcache.enable'); // false when the module is absent
//! ```

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::{Context, PhpString};

/// The kind of value PHP expects. The registry retains the original bytes.
#[derive(Clone, Copy)]
pub enum IniKind {
    Bool,
    Integer,
    Decimal,
    String,
    Quantity,
    List,
}

/// PHP permits runtime writes only when the user bit is set.
#[derive(Clone, Copy)]
pub enum IniAccess {
    User = 1,
    PerDirectory = 2,
    System = 4,
    PerDirectoryAndSystem = 6,
    All = 7,
}

impl IniAccess {
    pub fn runtime(self) -> bool {
        matches!(self, Self::User | Self::All)
    }
}

pub struct IniDefinition {
    pub module: &'static str,
    pub name: &'static str,
    pub default: Option<Vec<u8>>,
    pub kind: IniKind,
    pub access: IniAccess,
}

/// Apply an extension's runtime behavior before a change or restoration takes effect.
pub type IniUpdate = fn(&mut dyn Context, &str, Option<&[u8]>, &str) -> bool;

#[derive(Clone, Default)]
pub(crate) struct Catalog {
    names: HashMap<&'static [u8], usize>,
    definitions: Vec<&'static IniDefinition>,
    updates: HashMap<&'static str, IniUpdate>,
}

impl Catalog {
    pub(crate) fn register(&mut self, module: &str) {
        for entry in definitions()
            .iter()
            .filter(|entry| entry.module.eq_ignore_ascii_case(module))
        {
            self.names
                .insert(entry.name.as_bytes(), self.definitions.len());
            self.definitions.push(entry);
        }
    }

    pub(crate) fn update(&mut self, name: &'static str, update: IniUpdate) {
        self.updates.insert(name, update);
    }

    pub(crate) fn get(&self, name: &[u8]) -> Option<(usize, &'static IniDefinition)> {
        let index = *self.names.get(name)?;
        Some((index, *self.definitions.get(index)?))
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = (usize, &'static IniDefinition)> + '_ {
        self.definitions.iter().copied().enumerate()
    }

    pub(crate) fn hook(&self, name: &str) -> Option<IniUpdate> {
        self.updates.get(name).copied()
    }
}

fn definitions() -> &'static [IniDefinition] {
    static TABLE: OnceLock<Vec<IniDefinition>> = OnceLock::new();
    TABLE.get_or_init(|| {
        include_str!("../data/ini.tsv")
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(definition)
            .collect()
    })
}

fn definition(line: &'static str) -> Option<IniDefinition> {
    let mut columns = line.split('\t');
    let module = columns.next()?;
    let name = columns.next()?;
    let kind = match columns.next()? {
        "bool" => IniKind::Bool,
        "int" => IniKind::Integer,
        "decimal" => IniKind::Decimal,
        "string" => IniKind::String,
        "quantity" => IniKind::Quantity,
        "list" => IniKind::List,
        _ => return None,
    };
    let access = match columns.next()? {
        "1" => IniAccess::User,
        "2" => IniAccess::PerDirectory,
        "4" => IniAccess::System,
        "6" => IniAccess::PerDirectoryAndSystem,
        "7" => IniAccess::All,
        _ => return None,
    };
    let encoded = columns.next()?;
    let default = if encoded == "-" {
        None
    } else if encoded == "=" {
        Some(Vec::new())
    } else {
        let (pairs, remainder) = encoded.as_bytes().as_chunks::<2>();
        if !remainder.is_empty() {
            return None;
        }
        let bytes = pairs.iter().map(|pair| {
            let text = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(text, 16).ok()
        });
        Some(bytes.collect::<Option<Vec<_>>>()?)
    };
    Some(IniDefinition {
        module,
        name,
        default,
        kind,
        access,
    })
}

/// Parse the boolean spelling accepted by PHP's ordinary INI handlers.
pub fn boolean(bytes: &[u8]) -> bool {
    [b"on".as_slice(), b"yes", b"true"]
        .iter()
        .any(|word| bytes.eq_ignore_ascii_case(word))
        || integer(bytes) as i32 != 0
}

/// Decimal integer prefix used by PHP handlers that read the setting with atoi().
pub fn integer(bytes: &[u8]) -> i64 {
    let bytes = bytes.trim_ascii_start();
    let (negative, digits) = match bytes.split_first() {
        Some((b'-', tail)) => (true, tail),
        Some((b'+', tail)) => (false, tail),
        _ => (false, bytes),
    };
    digits
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .fold(0_i64, |value, digit| {
            let digit = i64::from(digit - b'0');
            let digit = if negative { -digit } else { digit };
            value.saturating_mul(10).saturating_add(digit)
        })
}

/// Resolve an explicit encoding override and then the request's default charset.
pub fn encoding(context: &dyn Context, option: &[u8]) -> PhpString {
    context
        .ini_get(option)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            context
                .ini_get(b"default_charset")
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "UTF-8".into())
}

mod quantity;
pub use quantity::parse_quantity;
