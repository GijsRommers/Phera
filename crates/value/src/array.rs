//! `ArrayKey` and `PhpArray`: PHP's ordered hash map.

use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
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
            Value::String(s) => Ok(string_to_key(s.as_bytes())),
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
fn string_to_key(bytes: &[u8]) -> ArrayKey {
    let not_a_key = || ArrayKey::String(PhpString::from(bytes));
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

#[derive(Clone, PartialEq, Eq, Hash)]
enum Idx {
    Int(i64),
    Str(u64),
}

fn key_idx(key: &ArrayKey) -> Idx {
    match key {
        ArrayKey::Int(i) => Idx::Int(*i),
        ArrayKey::String(s) => {
            // DefaultHasher (SipHash with fixed keys) is stable within a
            // process, which is all the index needs; collisions are resolved
            // by comparing the full stored keys.
            let mut h = std::collections::hash_map::DefaultHasher::new();
            s.as_bytes().hash(&mut h);
            Idx::Str(h.finish())
        }
    }
}

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
/// entries. A hash index gives O(1) average get/insert/remove. Tracks PHP's
/// next-free-integer-key rules and the internal array pointer.
#[derive(Clone, Default)]
pub struct PhpArray {
    entries: Vec<Entry>,
    /// Key discriminator -> positions of entries with it (usually one).
    index: HashMap<Idx, Vec<u32>>,
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
            index: HashMap::with_capacity(cap),
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

    fn drop_values(work: Vec<Value>) {
        let mut work = work;
        while let Some(v) = work.pop() {
            // Flatten a nested array when this Rc holds the last reference
            // to it; otherwise dropping is someone else's job.
            match v {
                Value::Array(rc) if Rc::strong_count(&rc) == 1 => {
                    if let Ok(mut inner) = Rc::try_unwrap(rc) {
                        work.extend(
                            std::mem::take(&mut inner.entries)
                                .into_iter()
                                .filter_map(|e| e.value),
                        );
                    }
                    // `inner` is empty here, so its own drop is trivial.
                }
                v => drop(v),
            }
        }
    }

    fn find_pos(&self, key: &ArrayKey) -> Option<usize> {
        let candidates = self.index.get(&key_idx(key))?;
        candidates
            .iter()
            .copied()
            .find(|&p| {
                let e = &self.entries[p as usize];
                e.value.is_some() && &e.key == key
            })
            .map(|p| p as usize)
    }

    pub fn get(&self, key: &ArrayKey) -> Option<&Value> {
        self.find_pos(key)
            .map(|p| self.entries[p].value.as_ref().unwrap())
    }

    pub fn get_mut(&mut self, key: &ArrayKey) -> Option<&mut Value> {
        self.find_pos(key)
            .map(|p| self.entries[p].value.as_mut().unwrap())
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
        self.entries.push(Entry {
            key,
            value: Some(value),
        });
        let pos = (self.entries.len() - 1) as u32;
        self.index
            .entry(key_idx(&self.entries[pos as usize].key))
            .or_default()
            .push(pos);
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
        let p = self.find_pos(key)?;
        self.dead += 1;
        let idx = key_idx(key);
        let cands = self.index.get_mut(&idx).unwrap();
        cands.retain(|&c| c != p as u32);
        if cands.is_empty() {
            self.index.remove(&idx);
        }
        self.entries[p].value.take()
    }

    /// Rebuilds entries/index when tombstones outnumber live entries.
    /// Positions held by outside iterators may shift across a compaction.
    fn maybe_compact(&mut self) {
        if self.dead > self.len() {
            let entries = std::mem::take(&mut self.entries);
            self.entries = entries.into_iter().filter(|e| e.value.is_some()).collect();
            self.dead = 0;
            self.index.clear();
            for (i, e) in self.entries.iter().enumerate() {
                self.index
                    .entry(key_idx(&e.key))
                    .or_default()
                    .push(i as u32);
            }
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
        let p = self.entries.iter().rposition(|e| e.value.is_some())?;
        let key = self.entries[p].key.clone();
        let value = self.entries[p].value.take().unwrap();
        self.dead += 1;
        let idx = key_idx(&key);
        let cands = self.index.get_mut(&idx).unwrap();
        cands.retain(|&c| c != p as u32);
        if cands.is_empty() {
            self.index.remove(&idx);
        }
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
        while matches!(self.entries.first().map(|e| &e.value), Some(None)) {
            self.entries.remove(0);
            self.dead -= 1;
        }
        let first = self.entries.first_mut()?;
        let key = first.key.clone();
        let value = first.value.take().unwrap();
        self.dead += 1;
        let idx = key_idx(&key);
        let cands = self.index.get_mut(&idx).unwrap();
        cands.retain(|&c| c != 0);
        if cands.is_empty() {
            self.index.remove(&idx);
        }
        // Renumber the remaining integer keys 0..n in order.
        let mut next = 0i64;
        for e in self.entries.iter_mut() {
            if e.value.is_some() && matches!(e.key, ArrayKey::Int(_)) {
                e.key = ArrayKey::Int(next);
                next += 1;
            }
        }
        self.rebuild_index();
        self.next_free = Some(next);
        self.pos = self.first_position().unwrap_or(INVALID);
        Some(value)
    }

    fn rebuild_index(&mut self) {
        self.index.clear();
        for (i, e) in self.entries.iter().enumerate() {
            if e.value.is_some() {
                self.index
                    .entry(key_idx(&e.key))
                    .or_default()
                    .push(i as u32);
            }
        }
    }

    /// True when the keys are exactly the integers 0..len-1 in order
    /// (PHP's `array_is_list`).
    pub fn is_list(&self) -> bool {
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
    /// Flattens the drop of nested arrays into a loop: a deeply nested
    /// structure must not recurse through Rc's drop glue.
    fn drop(&mut self) {
        Self::drop_values(
            std::mem::take(&mut self.entries)
                .into_iter()
                .filter_map(|e| e.value)
                .collect(),
        );
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
}
