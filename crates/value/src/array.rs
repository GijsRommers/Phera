//! `ArrayKey` and `PhpArray`: PHP's ordered hash map.

use std::collections::HashMap;

use foldhash::fast::RandomState;
use std::fmt;
use std::rc::Rc;

use crate::error::{Diagnostics, OpError};
use crate::string::PhpString;
use crate::value::Value;

/// An array key after PHP's normalisation: integer, or a string (used for
/// strings that are not canonical decimal integers in `i64` range).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ArrayKey {
    Int(i64),
    String(PhpString),
}

impl ArrayKey {
    pub fn int(i: i64) -> Self {
        ArrayKey::Int(i)
    }

    pub fn str(s: impl Into<PhpString>) -> Self {
        ArrayKey::String(s.into())
    }

    /// Normalises a PHP value into an array key, reporting deprecations and
    /// warnings to `diag` exactly as PHP 8.5 does.
    ///
    /// * ints stay, floats go through the `(int)` cast (out of range gives 0
    ///   with a warning, precision loss a deprecation);
    /// * `bool` becomes 0/1, `null` becomes `""` with a deprecation;
    /// * strings that are canonical decimal integers in range become ints
    ///   (no leading zeros, at most one leading `-`, no other characters);
    /// * arrays and objects are a TypeError, resources use their id.
    pub fn from_value_diag(value: &Value, diag: &mut dyn Diagnostics) -> Result<ArrayKey, OpError> {
        match &*value.deref() {
            Value::Int(i) => Ok(ArrayKey::Int(*i)),
            Value::Bool(b) => Ok(ArrayKey::Int(*b as i64)),
            Value::Float(f) => Ok(ArrayKey::Int(crate::convert::float_to_key_int(*f, diag))),
            Value::Null => {
                diag.deprecated(
                    "Using null as an array offset is deprecated, use an empty string instead"
                        .to_string(),
                );
                Ok(ArrayKey::str(""))
            }
            Value::String(s) => Ok(string_to_key(s)),
            Value::Array(_) => Err(OpError::type_error(
                "Cannot access offset of type array on array".to_string(),
            )),
            Value::Object(o) => Err(OpError::type_error(format!(
                "Cannot access offset of type {} on array",
                o.class.name()
            ))),
            Value::Resource(r) => {
                diag.warning(format!(
                    "Resource ID#{} used as offset, casting to integer ({})",
                    r.id, r.id
                ));
                Ok(ArrayKey::Int(r.id))
            }
            Value::Reference(_) => unreachable!("deref returns non-reference values"),
        }
    }

    /// Same as [`ArrayKey::from_value_diag`] with diagnostics discarded.
    pub fn from_value(value: &Value) -> Result<ArrayKey, OpError> {
        let mut n = crate::error::NoDiagnostics;
        Self::from_value_diag(value, &mut n)
    }
}

/// `zend_handle_numeric_str_ex`: canonical decimal integer key. Rejects
/// leading zeros (`"05"`, `"-0"`), `+`, whitespace, and anything that does
/// not fit in `i64` (`"9223372036854775808"` stays a string).
///
/// A key that stays a string shares the caller's string (one reference
/// count) instead of copying its bytes: `$row['name']` in a loop would
/// otherwise allocate a new string on every access.
fn string_to_key(string: &PhpString) -> ArrayKey {
    let bytes = string.as_bytes();
    let not_a_key = || ArrayKey::String(string.clone());
    let (digits, neg) = match bytes.split_first() {
        Some((b'-', rest)) => (rest, true),
        _ => (bytes, false),
    };
    // A leading zero (including "-0" and "" after the sign) never converts.
    // `length > 1` in C counts the whole key: "-0" and "00" never convert.
    if digits.is_empty() || (digits[0] == b'0' && bytes.len() > 1) || digits.len() > 19 {
        return not_a_key();
    }
    let mut acc: u64 = 0;
    for &b in digits {
        if !b.is_ascii_digit() {
            return not_a_key();
        }
        acc = acc * 10 + u64::from(b - b'0');
    }
    if neg {
        if acc > i64::MIN.unsigned_abs() {
            return not_a_key();
        }
        ArrayKey::Int((acc as i64).wrapping_neg())
    } else if acc > i64::MAX as u64 {
        not_a_key()
    } else {
        ArrayKey::Int(acc as i64)
    }
}

/// How a key is turned into a position in the entry vector.
///
/// Most PHP arrays are lists or small maps, and for those a hash table is
/// pure overhead: more memory, more allocation, and a hash computed for
/// every access. So the array picks the cheapest way that works and moves
/// to the next one only when it has to.
#[derive(Clone, Default)]
enum Lookup {
    /// A list: the entry at position `i` has the key `i`, so the key is the
    /// position. PHP calls this a packed array. A removed entry keeps its
    /// place as a hole, because a running `foreach` remembers positions and
    /// must not see a later element slide into one it has passed.
    #[default]
    Packed,
    /// A few entries with any keys: comparing them one by one is faster
    /// than hashing.
    Scan,
    /// Everything else: each live key with its position.
    Hashed(HashMap<ArrayKey, u32, RandomState>),
}

/// The most entries an array looks through one by one before it builds a
/// hash table.
const SCAN_LIMIT: usize = 8;

#[derive(Clone)]
struct Entry {
    key: ArrayKey,
    /// `None` marks a tombstone left behind by removal.
    value: Option<Value>,
}

/// Sentinel for the next-free index meaning "no push possible" (after an
/// `i64::MAX` key was inserted) and for the invalid internal-pointer state.
const INVALID: usize = usize::MAX;

/// PHP's ordered hash map.
///
/// Entries live in a vector (insertion order preserved); removals leave
/// tombstones and the map is compacted once tombstones outnumber live
/// entries. Keys are found through [`Lookup`]. Tracks PHP's
/// next-free-integer-key rules and the internal array pointer.
#[derive(Clone, Default)]
pub struct PhpArray {
    entries: Vec<Entry>,
    lookup: Lookup,
    dead: usize,
    /// PHP's `nNextFreeElement`: what the next `push` would use. `None`
    /// means no integer key was ever inserted (PHP starts at `LONG_MIN`,
    /// where `push` falls back to appending at 0). `Some(i64::MAX)` means
    /// `i64::MAX` is present and pushing is impossible.
    next_free: Option<i64>,
    /// Internal array pointer into `entries`; `INVALID` when unset.
    pos: usize,
}

impl PhpArray {
    pub fn new() -> Self {
        PhpArray::default()
    }

    pub fn with_capacity(cap: usize) -> Self {
        PhpArray {
            entries: Vec::with_capacity(cap),
            lookup: Lookup::Packed,
            dead: 0,
            next_free: None,
            pos: INVALID,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len() - self.dead
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drops values without recursing into what they contain.
    ///
    /// Dropping a value that is the last holder of an array, an object or a
    /// reference would normally drop its contents from inside its own drop,
    /// one stack frame per level. A million-node linked list would overflow
    /// the stack that way. Instead, the contents are moved onto this work
    /// list and dropped from the loop.
    fn drop_values(work: Vec<Value>) {
        let mut work = work;
        while let Some(value) = work.pop() {
            // Only the last holder frees the contents; for anyone else,
            // dropping just lowers the count.
            match value {
                Value::Array(rc) => {
                    if let Ok(mut inner) = Rc::try_unwrap(rc) {
                        work.extend(inner.take_values());
                    }
                }
                Value::Object(rc) => {
                    if let Ok(object) = Rc::try_unwrap(rc) {
                        work.extend(object.properties.into_inner().take_values());
                    }
                }
                Value::Reference(rc) => {
                    if let Ok(cell) = Rc::try_unwrap(rc) {
                        work.push(cell.into_inner());
                    }
                }
                scalar => drop(scalar),
            }
        }
    }

    /// Empties the array and hands back its values, leaving nothing for its
    /// own drop to do.
    fn take_values(&mut self) -> impl Iterator<Item = Value> + use<> {
        std::mem::take(&mut self.entries)
            .into_iter()
            .filter_map(|entry| entry.value)
    }

    fn find_pos(&self, key: &ArrayKey) -> Option<usize> {
        match &self.lookup {
            Lookup::Packed => match key {
                ArrayKey::Int(index) => usize::try_from(*index).ok().filter(|&position| {
                    self.entries
                        .get(position)
                        .is_some_and(|entry| entry.value.is_some())
                }),
                ArrayKey::String(_) => None,
            },
            Lookup::Scan => self
                .entries
                .iter()
                .position(|entry| entry.value.is_some() && &entry.key == key),
            Lookup::Hashed(positions) => positions.get(key).map(|&position| position as usize),
        }
    }

    /// The lookup that fits the entries as they are now.
    fn fresh_lookup(&self) -> Lookup {
        let is_list = self.dead == 0
            && self
                .entries
                .iter()
                .enumerate()
                .all(|(position, entry)| entry.key == ArrayKey::Int(position as i64));
        if is_list {
            Lookup::Packed
        } else {
            self.lookup_by_size()
        }
    }

    /// The lookup for entries that are known not to form a list.
    fn lookup_by_size(&self) -> Lookup {
        if self.entries.len() <= SCAN_LIMIT {
            return Lookup::Scan;
        }
        let positions = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.value.is_some())
            .map(|(position, entry)| (entry.key.clone(), position as u32))
            .collect();
        Lookup::Hashed(positions)
    }

    /// Keeps the lookup right after the entry with this key lost its value.
    fn after_removal(&mut self, key: &ArrayKey) {
        // Packed and scanned arrays find a hole by looking at the entry.
        if let Lookup::Hashed(positions) = &mut self.lookup {
            positions.remove(key);
        }
    }

    pub fn get(&self, key: &ArrayKey) -> Option<&Value> {
        let position = self.find_pos(key)?;
        self.entries.get(position)?.value.as_ref()
    }

    pub fn get_mut(&mut self, key: &ArrayKey) -> Option<&mut Value> {
        let position = self.find_pos(key)?;
        self.entries.get_mut(position)?.value.as_mut()
    }

    pub fn contains_key(&self, key: &ArrayKey) -> bool {
        self.find_pos(key).is_some()
    }

    /// Sets `key` to `value`; returns the previous value if the key existed.
    pub fn insert(&mut self, key: ArrayKey, value: Value) -> Option<Value> {
        if let ArrayKey::Int(i) = &key {
            self.note_int_key(*i);
        }
        if let Some(p) = self.find_pos(&key) {
            return self.entries[p].value.replace(value);
        }
        self.push_entry(key, value);
        None
    }

    /// `if (h >= nNextFreeElement) nNextFreeElement = h+1 (capped at MAX)`;
    /// every integer key, negative ones included, participates (PHP >= 8.3).
    fn note_int_key(&mut self, h: i64) {
        if self.next_free.is_none_or(|n| h >= n) {
            self.next_free = if h == i64::MAX {
                Some(i64::MAX)
            } else {
                Some(h + 1)
            };
        }
    }

    fn push_entry(&mut self, key: ArrayKey, value: Value) {
        let position = self.entries.len();
        let extends_list =
            matches!(self.lookup, Lookup::Packed) && key == ArrayKey::Int(position as i64);
        if !extends_list {
            match &mut self.lookup {
                Lookup::Hashed(positions) => {
                    positions.insert(key.clone(), position as u32);
                }
                // Decided after the push, when the entry is in place.
                Lookup::Packed | Lookup::Scan => {}
            }
        }
        self.entries.push(Entry {
            key,
            value: Some(value),
        });
        if !extends_list && !matches!(self.lookup, Lookup::Hashed(_)) {
            self.lookup = self.lookup_by_size();
        }
        self.maybe_compact();
    }

    /// Appends like `$a[] = $value`, using PHP's next-free-integer-key rules.
    /// Pushing once an `i64::MAX` key exists is the error PHP throws.
    pub fn push(&mut self, value: Value) -> Result<ArrayKey, OpError> {
        let key = self.next_free.unwrap_or(0);
        if self.next_free == Some(i64::MAX) {
            // The next slot is occupied (or unreachable), as in PHP.
            return Err(OpError::error(
                "Cannot add element to the array as the next element is already occupied",
            ));
        }
        self.note_int_key(key);
        self.push_entry(ArrayKey::Int(key), value);
        Ok(ArrayKey::Int(key))
    }

    /// Removes `key`, returning its value. Tombstones are compacted lazily.
    pub fn remove(&mut self, key: &ArrayKey) -> Option<Value> {
        let position = self.find_pos(key)?;
        let value = self.entries.get_mut(position)?.value.take();
        self.dead += 1;
        self.after_removal(key);
        value
    }

    /// Rebuilds entries/index when tombstones outnumber live entries.
    /// Positions held by outside iterators may shift across a compaction.
    fn maybe_compact(&mut self) {
        if self.dead > self.len() {
            self.entries.retain(|entry| entry.value.is_some());
            self.dead = 0;
            self.lookup = self.fresh_lookup();
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ArrayKey, &Value)> {
        self.entries
            .iter()
            .filter_map(|e| e.value.as_ref().map(|v| (&e.key, v)))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&ArrayKey, &mut Value)> {
        self.entries.iter_mut().filter_map(|e| {
            let key = &e.key;
            e.value.as_mut().map(|v| (key, v))
        })
    }

    pub fn keys(&self) -> impl Iterator<Item = &ArrayKey> {
        self.entries
            .iter()
            .filter_map(|e| e.value.as_ref().map(|_| &e.key))
    }

    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.entries.iter().filter_map(|e| e.value.as_ref())
    }

    pub fn first(&self) -> Option<(&ArrayKey, &Value)> {
        self.iter().next()
    }

    pub fn last(&self) -> Option<(&ArrayKey, &Value)> {
        self.entries
            .iter()
            .rev()
            .find_map(|e| e.value.as_ref().map(|v| (&e.key, v)))
    }

    /// Removes and returns the last entry, like `array_pop`. When the popped
    /// key was one below the next-free index, PHP rolls that index back one
    /// step.
    pub fn pop(&mut self) -> Option<Value> {
        let position = self
            .entries
            .iter()
            .rposition(|entry| entry.value.is_some())?;
        let entry = self.entries.get_mut(position)?;
        let key = entry.key.clone();
        let value = entry.value.take()?;
        self.dead += 1;
        self.after_removal(&key);
        if let (ArrayKey::Int(k), Some(n)) = (&key, self.next_free)
            && n == k.checked_add(1).unwrap_or(i64::MAX)
        {
            self.next_free = Some(*k);
        }
        self.maybe_compact();
        Some(value)
    }

    /// Removes and returns the first entry, like `array_shift`: all integer
    /// keys of the remainder are renumbered 0..n in order and the next-free
    /// index becomes that count.
    pub fn shift(&mut self) -> Option<Value> {
        let first = self.first_position()?;
        let value = self.entries.get_mut(first)?.value.take();
        // Every integer key changes anyway, so close all holes: a list
        // stays a list.
        self.entries.retain(|entry| entry.value.is_some());
        self.dead = 0;
        // Renumber the remaining integer keys 0..n in order.
        let mut next = 0i64;
        for entry in self.entries.iter_mut() {
            if entry.value.is_some() && matches!(entry.key, ArrayKey::Int(_)) {
                entry.key = ArrayKey::Int(next);
                next += 1;
            }
        }
        self.lookup = self.fresh_lookup();
        self.next_free = Some(next);
        self.pos = self.first_position().unwrap_or(INVALID);
        value
    }

    /// Whether the keys are exactly 0, 1, 2, ... in order.
    pub fn is_list(&self) -> bool {
        if matches!(self.lookup, Lookup::Packed) && self.dead == 0 {
            return true;
        }
        let mut expect = 0i64;
        for (k, _) in self.iter() {
            match k {
                ArrayKey::Int(i) if *i == expect => expect += 1,
                _ => return false,
            }
        }
        true
    }

    /// The key the next `push` would use (PHP's `nNextFreeElement`); `None`
    /// would mean pushing is impossible, which only happens once an
    /// `i64::MAX` key exists.
    pub fn next_free_index(&self) -> Option<i64> {
        self.next_free
    }

    // -- positional iteration (valid across modification, modulo compaction) --

    pub fn first_position(&self) -> Option<usize> {
        self.entries.iter().position(|e| e.value.is_some())
    }

    pub fn next_position(&self, pos: usize) -> Option<usize> {
        match pos.checked_add(1) {
            Some(start) if start < self.entries.len() => self.entries[start..]
                .iter()
                .position(|e| e.value.is_some())
                .map(|o| o + start),
            _ => None,
        }
    }

    pub fn entry_at(&self, pos: usize) -> Option<(&ArrayKey, &Value)> {
        self.entries
            .get(pos)
            .and_then(|e| e.value.as_ref().map(|v| (&e.key, v)))
    }

    pub fn entry_at_mut(&mut self, pos: usize) -> Option<(&ArrayKey, &mut Value)> {
        match self.entries.get_mut(pos) {
            Some(e) => {
                let key = &e.key;
                e.value.as_mut().map(|v| (key, v))
            }
            None => None,
        }
    }

    // -- internal array pointer --

    fn valid_pos(&self) -> Option<usize> {
        match self.entries.get(self.pos) {
            Some(e) if e.value.is_some() => Some(self.pos),
            _ => None,
        }
    }

    /// `current()`: the value at the internal pointer.
    pub fn current(&self) -> Option<&Value> {
        self.valid_pos()
            .map(|p| self.entries[p].value.as_ref().unwrap())
    }

    /// `key()`: the key at the internal pointer.
    pub fn key(&self) -> Option<&ArrayKey> {
        self.valid_pos().map(|p| &self.entries[p].key)
    }

    /// `next()`: move the pointer to the following entry. From the invalid
    /// state this moves to the first entry (matching PHP after a `prev()`
    /// past the start).
    pub fn advance(&mut self) {
        let start = self.valid_pos().unwrap_or(INVALID);
        let from = start.checked_add(1).unwrap_or(INVALID);
        self.pos = if from == INVALID {
            self.first_position().unwrap_or(INVALID)
        } else {
            self.entries[from..]
                .iter()
                .position(|e| e.value.is_some())
                .map_or(INVALID, |o| o + from)
        };
    }

    /// `prev()`: move the pointer to the previous entry; at the start of the
    /// array (or from an invalid pointer) this invalidates the pointer.
    pub fn rewind_back(&mut self) {
        self.pos = match self.valid_pos() {
            Some(p) => p
                .checked_sub(1)
                .and_then(|limit| {
                    self.entries[..=limit]
                        .iter()
                        .rposition(|e| e.value.is_some())
                })
                .unwrap_or(INVALID),
            None => INVALID,
        };
    }

    /// `reset()`: pointer to the first entry.
    pub fn reset(&mut self) {
        self.pos = self.first_position().unwrap_or(INVALID);
    }

    /// `end()`: pointer to the last entry.
    pub fn end(&mut self) {
        self.pos = self
            .entries
            .iter()
            .rposition(|e| e.value.is_some())
            .unwrap_or(INVALID);
    }
}

impl Drop for PhpArray {
    /// Flattens the drop of everything nested inside into a loop: a deeply
    /// nested structure must not recurse through Rc's drop glue.
    fn drop(&mut self) {
        Self::drop_values(self.take_values().collect());
    }
}

impl fmt::Debug for PhpArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl FromIterator<(ArrayKey, Value)> for PhpArray {
    fn from_iter<T: IntoIterator<Item = (ArrayKey, Value)>>(iter: T) -> Self {
        let iter = iter.into_iter();
        let mut a = PhpArray::with_capacity(iter.size_hint().0);
        for (k, v) in iter {
            a.insert(k, v);
        }
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CollectDiagnostics;
    use crate::object::{ClassHandle, Object};
    use std::rc::Rc;

    fn k(i: i64) -> ArrayKey {
        ArrayKey::Int(i)
    }
    fn ks(s: &str) -> ArrayKey {
        // Normalise exactly like `$a[$s] = ...` does in PHP.
        ArrayKey::from_value(&Value::String(PhpString::from(s))).unwrap()
    }
    fn vi(i: i64) -> Value {
        Value::Int(i)
    }

    #[test]
    fn key_normalisation() {
        let t = |s: &str| ArrayKey::from_value(&Value::String(PhpString::from(s))).unwrap();
        assert_eq!(t("5"), k(5));
        assert_eq!(t("-5"), k(-5));
        assert_eq!(t("0"), k(0));
        assert_eq!(t("9223372036854775807"), k(i64::MAX));
        assert_eq!(t("-9223372036854775808"), k(i64::MIN));
        for s in [
            "05",
            "5.0",
            " 5",
            "+5",
            "-0",
            " 5 ",
            "08",
            "1e3",
            "9223372036854775808",
            "-",
            "",
        ] {
            assert_eq!(t(s), ks(s), "key {s:?} must stay a string");
        }
        // int/bool/null/float
        assert_eq!(ArrayKey::from_value(&Value::Int(-7)).unwrap(), k(-7));
        assert_eq!(ArrayKey::from_value(&Value::Bool(true)).unwrap(), k(1));
        assert_eq!(ArrayKey::from_value(&Value::Bool(false)).unwrap(), k(0));
        assert_eq!(ArrayKey::from_value(&Value::Null).unwrap(), ks(""));
        assert_eq!(ArrayKey::from_value(&Value::Float(5.0)).unwrap(), k(5));
        assert_eq!(ArrayKey::from_value(&Value::Float(-5.9)).unwrap(), k(-5));
        // arrays and objects are TypeErrors
        let arr = Value::Array(Rc::new(PhpArray::new()));
        assert_eq!(
            ArrayKey::from_value(&arr).unwrap_err().message,
            "Cannot access offset of type array on array"
        );
        let class: Rc<dyn ClassHandle> = Rc::new(DummyClass {
            name: "stdClass".into(),
        });
        let obj = Value::Object(Rc::new(Object::new(1, class)));
        assert_eq!(
            ArrayKey::from_value(&obj).unwrap_err().message,
            "Cannot access offset of type stdClass on array"
        );
    }

    struct DummyClass {
        name: String,
    }
    impl ClassHandle for DummyClass {
        fn name(&self) -> &str {
            &self.name
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[test]
    fn null_key_deprecation() {
        let mut d = CollectDiagnostics::default();
        ArrayKey::from_value_diag(&Value::Null, &mut d).unwrap();
        assert_eq!(
            d.messages(),
            vec!["Using null as an array offset is deprecated, use an empty string instead"]
        );
    }

    #[test]
    fn order_preserved_after_removals_and_compaction() {
        let mut a = PhpArray::new();
        for i in 0..40 {
            a.insert(k(i * 2), vi(i));
        }
        // Remove every even-indexed key to create many tombstones.
        for i in 0..40 {
            if i % 2 == 1 {
                let removed = a.remove(&k(i * 2));
                assert!(removed.is_some());
            }
        }
        assert_eq!(a.len(), 20);
        let keys: Vec<i64> = a
            .keys()
            .filter_map(|kk| {
                if let ArrayKey::Int(i) = kk {
                    Some(*i)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            keys,
            (0..40)
                .filter(|i| i % 2 == 0)
                .map(|i| i * 2)
                .collect::<Vec<_>>()
        );
        // Keep removing until compaction definitely kicks in (dead > live).
        for i in 0..40 {
            if i % 2 == 0 && i > 10 {
                a.remove(&k(i * 2));
            }
        }
        assert_eq!(a.len(), 6);
        let vals: Vec<i64> = a
            .values()
            .filter_map(|v| {
                if let Value::Int(i) = v {
                    Some(*i)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(vals, vec![0, 2, 4, 6, 8, 10]);
    }

    #[test]
    fn next_free_rules() {
        // Removals do not rewind the next-free index...
        let mut a = PhpArray::new();
        a.insert(k(5), vi(1));
        a.remove(&k(5));
        assert_eq!(a.next_free_index(), Some(6));
        // ...except through pop's roll-back rule (covered below); and an
        // emptied array keeps counting from the largest key ever seen.
        let _ = a.remove(&k(6));
        a.push(vi(9)).unwrap();
        assert_eq!(a.get(&k(6)), Some(&vi(9)));
    }

    #[test]
    fn next_free_exact_php_rules() {
        // [5=>'x', 2=>'y'] push -> 6
        let mut a = PhpArray::from_iter([(k(5), vi(1)), (k(2), vi(2))]);
        assert_eq!(a.push(vi(3)).unwrap(), k(6));
        // [-5] push -> -4
        let mut a = PhpArray::new();
        a.insert(k(-5), vi(1));
        assert_eq!(a.push(vi(2)).unwrap(), k(-4));
        // [-5, 3] push -> 4
        a.insert(k(3), vi(3));
        assert_eq!(a.push(vi(4)).unwrap(), k(4));
        // ["5","6"] push -> 7
        let mut a = PhpArray::new();
        a.insert(ks("5"), vi(1));
        a.insert(ks("6"), vi(2));
        assert_eq!(a.push(vi(3)).unwrap(), k(7));
        // [i64::MAX] push -> error
        let mut a = PhpArray::new();
        a.insert(k(i64::MAX), vi(1));
        let e = a.push(vi(2)).unwrap_err();
        assert_eq!(
            e.message,
            "Cannot add element to the array as the next element is already occupied"
        );
        // [i64::MIN] push -> i64::MIN + 1
        let mut a = PhpArray::new();
        a.insert(k(i64::MIN), vi(1));
        assert_eq!(a.push(vi(2)).unwrap(), k(i64::MIN + 1));
        // [1=>'a'] unset -> push -> 2 ; ['x'] unset -> push -> 0
        let mut a = PhpArray::from_iter([(k(1), vi(1))]);
        a.remove(&k(1));
        assert_eq!(a.push(vi(1)).unwrap(), k(2));
        let mut a = PhpArray::from_iter([(ks("x"), vi(1))]);
        a.remove(&ks("x"));
        assert_eq!(a.push(vi(1)).unwrap(), k(0));
        // pop resets to 0 on a fresh-int array: [5] pop -> push 5
        let mut a = PhpArray::from_iter([(k(5), vi(1))]);
        a.pop();
        assert_eq!(a.push(vi(1)).unwrap(), k(5));
        // pop rolls back: [5, 6] two pops -> push 5
        let mut a = PhpArray::from_iter([(k(5), vi(1)), (k(6), vi(2))]);
        a.pop();
        a.pop();
        assert_eq!(a.push(vi(1)).unwrap(), k(5));
    }

    #[test]
    fn push_overflow_is_an_error() {
        let mut a = PhpArray::with_capacity(2);
        a.insert(k(i64::MAX), vi(1));
        assert_eq!(a.next_free_index(), Some(i64::MAX));
        assert_eq!(
            a.push(vi(2)).unwrap_err().message,
            "Cannot add element to the array as the next element is already occupied"
        );
    }

    #[test]
    fn shift_and_pop_reindexing() {
        // [10'a', 'x''b', 5'c'] shift -> x'b', 0'c'
        let mut a = PhpArray::from_iter([(k(10), vi(1)), (ks("x"), vi(2)), (k(5), vi(3))]);
        let shifted = a.shift().unwrap();
        assert_eq!(shifted, vi(1));
        let got: Vec<(ArrayKey, i64)> = a
            .iter()
            .map(|(kk, v)| {
                (
                    kk.clone(),
                    if let Value::Int(i) = v { *i } else { panic!() },
                )
            })
            .collect();
        assert_eq!(got, vec![(ks("x"), 2), (k(0), 3)]);
        assert_eq!(a.next_free_index(), Some(1));
        // ['x''a', 5'b', 'y''c'] shift -> 0'b', 'y''c'
        let mut a = PhpArray::from_iter([(ks("x"), vi(1)), (k(5), vi(2)), (ks("y"), vi(3))]);
        a.shift();
        let got: Vec<ArrayKey> = a.keys().cloned().collect();
        assert_eq!(got, vec![k(0), ks("y")]);
        // array_pop does not reindex
        let mut a = PhpArray::from_iter([(k(10), vi(1)), (ks("x"), vi(2)), (k(5), vi(3))]);
        a.pop();
        let got: Vec<ArrayKey> = a.keys().cloned().collect();
        assert_eq!(got, vec![k(10), ks("x")]);
    }

    #[test]
    fn positional_iteration_under_modification() {
        let mut a = PhpArray::from_iter([(k(0), vi(10)), (k(1), vi(20)), (k(2), vi(30))]);
        let mut pos = a.first_position().unwrap();
        // Mutate while iterating positions.
        if let Some((_, v)) = a.entry_at_mut(pos) {
            *v = vi(11);
        }
        pos = a.next_position(pos).unwrap();
        assert_eq!(a.entry_at(pos), Some((&k(1), &vi(20))));
        a.remove(&k(1));
        // The removed slot is skipped.
        pos = a.next_position(pos).unwrap();
        assert_eq!(a.entry_at(pos), Some((&k(2), &vi(30))));
        assert_eq!(a.next_position(pos), None);
        assert_eq!(a.first(), Some((&k(0), &vi(11))));
        assert_eq!(a.last(), Some((&k(2), &vi(30))));
    }

    #[test]
    fn internal_pointer() {
        let mut a = PhpArray::from_iter([(ks("a"), vi(1)), (ks("b"), vi(2)), (ks("c"), vi(3))]);
        assert_eq!(a.current(), None); // pointer unset until reset
        a.reset();
        assert_eq!(a.current(), Some(&vi(1)));
        assert_eq!(a.key(), Some(&ks("a")));
        a.advance();
        assert_eq!(a.current(), Some(&vi(2)));
        a.end();
        assert_eq!(a.current(), Some(&vi(3)));
        a.rewind_back();
        assert_eq!(a.current(), Some(&vi(2)));
        a.rewind_back();
        a.rewind_back(); // past the start
        assert_eq!(a.current(), None);
        a.advance();
        assert_eq!(a.current(), Some(&vi(1)));
        a.reset();
        assert_eq!(a.current(), Some(&vi(1)));
    }

    #[test]
    fn copy_on_write_arrays() {
        use crate::value::Value;
        let shared = Rc::new(PhpArray::from_iter([(k(0), vi(1))]));
        let a = Value::Array(shared.clone());
        let b = a.clone();
        // Mutate through one handle.
        let Value::Array(r) = &a else { panic!() };
        let mut php_array = (**r).clone();
        php_array.push(vi(2)).unwrap();
        // Proper COW goes through Rc::make_mut on the Rc:
        let Value::Array(r2) = &b else { panic!() };
        let mut cow = r2.clone();
        Rc::make_mut(&mut cow).insert(k(9), vi(9));
        assert_eq!((**r2).len(), 1);
        assert_eq!(cow.len(), 2);
        assert_eq!((**r).len(), 1);
    }

    #[test]
    fn is_list() {
        let mut a = PhpArray::from_iter([(k(0), vi(1)), (k(1), vi(2))]);
        assert!(a.is_list());
        a.insert(k(-1), vi(3));
        assert!(!a.is_list());
        let mut a = PhpArray::new();
        assert!(a.is_list());
        a.insert(ks("z"), vi(1));
        assert!(!a.is_list());
    }

    #[test]
    fn mut_iterate_and_defaults() {
        let mut a = PhpArray::from_iter([(k(0), vi(1)), (k(1), vi(2))]);
        for (_, v) in a.iter_mut() {
            if let Value::Int(i) = v {
                *i *= 10;
            }
        }
        assert_eq!(a.get(&k(1)), Some(&vi(20)));
        assert!(a.contains_key(&k(0)));
        assert!(!a.contains_key(&k(5)));
        assert_eq!(a.len(), 2);
        assert!(!a.is_empty());
        let _ = PhpString::default();
    }

    /// Which lookup an array uses is invisible from outside, so these tests
    /// look inside.
    fn lookup_name(array: &PhpArray) -> &'static str {
        match array.lookup {
            Lookup::Packed => "packed",
            Lookup::Scan => "scan",
            Lookup::Hashed(_) => "hashed",
        }
    }

    #[test]
    fn a_list_needs_no_index_however_long() {
        let mut list = PhpArray::new();
        for value in 0..10_000 {
            list.push(vi(value)).unwrap();
        }
        assert_eq!(lookup_name(&list), "packed");
        assert_eq!(list.get(&k(9_999)), Some(&vi(9_999)));
        assert_eq!(list.get(&k(10_000)), None);
        assert_eq!(list.get(&k(-1)), None);
        assert_eq!(list.get(&ks("five")), None);

        // Removing entries leaves holes, not an index.
        assert_eq!(list.pop(), Some(vi(9_999)));
        assert_eq!(list.remove(&k(5)), Some(vi(5)));
        assert_eq!(lookup_name(&list), "packed");
        assert_eq!(list.get(&k(5)), None);
        assert_eq!(list.get(&k(6)), Some(&vi(6)));
        assert!(!list.is_list());

        // Shifting renumbers everything, which makes it a list again.
        assert_eq!(list.shift(), Some(vi(0)));
        assert_eq!(lookup_name(&list), "packed");
        assert!(list.is_list());
        assert_eq!(list.get(&k(0)), Some(&vi(1)));
        assert_eq!(list.get(&k(4)), Some(&vi(6)));

        // A key out of order ends it: PHP appends it, it does not slot in.
        list.remove(&k(2));
        list.insert(k(2), vi(-1));
        assert_eq!(lookup_name(&list), "hashed");
        assert_eq!(list.last(), Some((&k(2), &vi(-1))));
    }

    #[test]
    fn small_maps_are_scanned_and_large_ones_hashed() {
        let mut map = PhpArray::new();
        for name in ["a", "b", "c", "d", "e", "f", "g", "h"] {
            map.insert(ks(name), Value::from(name));
        }
        assert_eq!(lookup_name(&map), "scan");
        assert_eq!(map.get(&ks("h")), Some(&Value::from("h")));

        map.insert(ks("i"), Value::from("i"));
        assert_eq!(lookup_name(&map), "hashed");
        for name in ["a", "e", "i"] {
            assert_eq!(map.get(&ks(name)), Some(&Value::from(name)));
        }
        assert_eq!(map.get(&ks("z")), None);
    }

    /// Random inserts, pushes and removals, checked after every step
    /// against a plain list of pairs that does the same thing the slow,
    /// obvious way. The key space is small so that arrays keep crossing
    /// between the three lookups and keep being compacted.
    #[test]
    fn every_lookup_agrees_with_a_plain_list_of_pairs() {
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut random = move |bound: u64| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) % bound
        };
        let mut array = PhpArray::new();
        let mut pairs: Vec<(ArrayKey, Value)> = Vec::new();

        for step in 0..60_000_i64 {
            let key = if random(3) == 0 {
                ks(["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l"]
                    [random(12) as usize])
            } else {
                k(random(24) as i64 - 4)
            };
            match random(10) {
                0..=3 => {
                    let value = vi(step);
                    array.insert(key.clone(), value.clone());
                    match pairs.iter_mut().find(|(existing, _)| existing == &key) {
                        Some(pair) => pair.1 = value,
                        None => pairs.push((key, value)),
                    }
                }
                4..=5 => {
                    if let Ok(pushed) = array.push(vi(step)) {
                        assert!(pairs.iter().all(|(existing, _)| existing != &pushed));
                        pairs.push((pushed, vi(step)));
                    }
                }
                6..=7 => {
                    let expected = pairs
                        .iter()
                        .position(|(existing, _)| existing == &key)
                        .map(|position| pairs.remove(position).1);
                    assert_eq!(array.remove(&key), expected);
                }
                8 => assert_eq!(array.pop(), pairs.pop().map(|pair| pair.1)),
                _ => {
                    let expected = (!pairs.is_empty()).then(|| pairs.remove(0).1);
                    assert_eq!(array.shift(), expected);
                    let mut next = 0;
                    for (existing, _) in pairs.iter_mut() {
                        if matches!(existing, ArrayKey::Int(_)) {
                            *existing = k(next);
                            next += 1;
                        }
                    }
                }
            }

            assert_eq!(array.len(), pairs.len(), "step {step}");
            let seen: Vec<(ArrayKey, Value)> = array
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            assert_eq!(seen, pairs, "step {step}");
            for (key, value) in &pairs {
                assert_eq!(array.get(key), Some(value), "step {step}, key {key:?}");
            }
            assert_eq!(array.get(&k(1_000)), None);
            assert_eq!(array.get(&ks("absent")), None);
        }
    }
}
