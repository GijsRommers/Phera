//! Every lowercased name the process has compiled, as one number.
//!
//! ```php
//! $point->add(1, 2);    // a.php: "add" is name 3 of the file
//! $list->add($item);    // b.php: "add" is name 7 of the file
//! // both are NameKey(42), in every request of every worker
//! ```
//!
//! A file's names get their keys once, when the file is compiled
//! ([`crate::UnitMetadata`]), and the keys are kept with the compiled file.
//! A request that loads the file uses them as they are: it hashes no name.
//! Method tables are keyed by these numbers, so a class linked in one
//! request has a table that is valid in every other. Keys are never
//! reused: a name keeps its number for the life of the process.

use foldhash::HashMap;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

/// A lowercased name's number in the process.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct NameKey(pub u32);

/// A case-sensitive property name's number in the process.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct PropKey(pub u32);

/// The numbers given so far. Only compiling a file and building a method
/// table take the lock, never the interpreter.
static KEYS: LazyLock<Mutex<HashMap<Box<str>, NameKey>>> = LazyLock::new(Mutex::default);
static PROP_KEYS: LazyLock<Mutex<HashMap<Box<str>, PropKey>>> = LazyLock::new(Mutex::default);
static KEY_SELF: LazyLock<NameKey> = LazyLock::new(|| key("self"));
static KEY_STATIC: LazyLock<NameKey> = LazyLock::new(|| key("static"));
static KEY_PARENT: LazyLock<NameKey> = LazyLock::new(|| key("parent"));

/// Process-wide key for `"self"`.
#[inline]
pub fn key_self() -> NameKey {
    *KEY_SELF
}

/// Process-wide key for `"static"`.
#[inline]
pub fn key_static() -> NameKey {
    *KEY_STATIC
}

/// Process-wide key for `"parent"`.
#[inline]
pub fn key_parent() -> NameKey {
    *KEY_PARENT
}

/// The key of `lowercase`, numbering it if no compiled file used it yet.
pub fn key(lowercase: &str) -> NameKey {
    let mut keys = KEYS.lock().unwrap_or_else(PoisonError::into_inner);
    number(&mut keys, lowercase)
}

/// The keys of `names`, in order, under one lock.
pub fn keys<'a>(names: impl IntoIterator<Item = &'a str>) -> Arc<[NameKey]> {
    let mut keys = KEYS.lock().unwrap_or_else(PoisonError::into_inner);
    names
        .into_iter()
        .map(|name| number(&mut keys, name))
        .collect()
}

/// The case-sensitive property keys of `names`, in order, under one lock.
pub fn prop_keys<'a>(names: impl IntoIterator<Item = &'a str>) -> Arc<[PropKey]> {
    let mut keys = PROP_KEYS.lock().unwrap_or_else(PoisonError::into_inner);
    names
        .into_iter()
        .map(|name| number_prop(&mut keys, name))
        .collect()
}

fn number(keys: &mut HashMap<Box<str>, NameKey>, lowercase: &str) -> NameKey {
    if let Some(key) = keys.get(lowercase) {
        return *key;
    }
    // Reserve u32::MAX as empty sentinel in flat open-addressed method tables.
    let next = NameKey(u32::try_from(keys.len()).unwrap_or(u32::MAX - 1));
    keys.insert(lowercase.into(), next);
    next
}

fn number_prop(keys: &mut HashMap<Box<str>, PropKey>, name: &str) -> PropKey {
    if let Some(key) = keys.get(name) {
        return *key;
    }
    // Reserve u32::MAX as empty sentinel in flat open-addressed property tables.
    let next = PropKey(u32::try_from(keys.len()).unwrap_or(u32::MAX - 1));
    keys.insert(name.into(), next);
    next
}

#[cfg(test)]
mod tests {
    use super::{key, keys};

    #[test]
    fn a_name_has_one_key_for_the_process() {
        let first = keys(["phera_name_keys_add", "phera_name_keys_get"]);
        let again = keys(["phera_name_keys_get", "phera_name_keys_add"]);
        assert_eq!(first[0], again[1]);
        assert_eq!(first[1], again[0]);
        assert_ne!(first[0], first[1]);
        assert_eq!(key("phera_name_keys_add"), first[0]);
    }
}
