//! Immortal deduplicated PHP strings with pre-computed array hash and
//! numeric-key classification.
//!
//! Compile-time literals, file names, class/method/property/constant names,
//! and constant array keys are interned once per process (up to a fixed memory
//! cap, matching Zend's `opcache.interned_strings_buffer`). Because every
//! `&'static InternedStr` comes from the single deduplicating table, pointer
//! equality (`std::ptr::eq`) is exact byte equality, cloning and dropping
//! `Value::InternedStr` and `ArrayKey::Interned` do no reference counting, and
//! array lookups reuse the pre-computed hash and numeric-key classification.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::{Mutex, OnceLock};

use hashbrown::HashTable;

use crate::array::{ArrayKey, hash_str_bytes, numeric_string_key};
use crate::string::PhpString;
use crate::value::Value;

/// Maximum number of unique strings held in the process-wide interner.
const MAX_INTERNED_STRINGS: usize = 131_072;
/// Maximum total byte payload held in the process-wide interner (16 MiB).
const MAX_INTERNED_BYTES: usize = 16 * 1024 * 1024;
/// Maximum length of a single string eligible for interning.
const MAX_SINGLE_INTERN_LEN: usize = 4_096;

/// An immutable, deduplicated PHP byte string with pre-computed metadata.
pub struct InternedStr {
    hash: u64,
    numeric: i64,
    bytes: &'static [u8],
    is_numeric: bool,
    truthy: bool,
}

impl InternedStr {
    /// Interns `bytes` in the process-wide table, returning a canonical
    /// `&'static InternedStr`, or `None` if `bytes` is too long or the table
    /// cap has been reached.
    #[inline]
    pub fn try_intern(bytes: &[u8]) -> Option<&'static InternedStr> {
        if bytes.is_empty() {
            return Some(Self::empty());
        }
        if bytes.len() > MAX_SINGLE_INTERN_LEN {
            return None;
        }
        intern_non_empty(bytes)
    }

    /// The canonical empty interned string, available without locking.
    #[inline]
    pub fn empty() -> &'static InternedStr {
        static EMPTY: OnceLock<&'static InternedStr> = OnceLock::new();
        EMPTY.get_or_init(|| {
            let mut table = interner().lock().expect("interner poisoned");
            table.intern_unchecked(hash_str_bytes(b""), b"")
        })
    }

    #[inline(always)]
    pub fn as_bytes(&self) -> &'static [u8] {
        self.bytes
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Pre-computed hash under the process-wide array hash seed.
    #[inline(always)]
    pub(crate) fn array_hash(&self) -> u64 {
        self.hash
    }

    /// Canonical PHP decimal integer key if this string normalises to one.
    #[inline(always)]
    pub fn numeric_key(&self) -> Option<i64> {
        self.is_numeric.then_some(self.numeric)
    }

    /// Pre-computed PHP truthiness (`!(empty || "0")`).
    #[inline(always)]
    pub fn is_truthy(&self) -> bool {
        self.truthy
    }

    /// Normalises this string into a PHP array key with no digit scan or
    /// allocation.
    #[inline(always)]
    pub fn to_array_key(&'static self) -> ArrayKey {
        if self.is_numeric {
            ArrayKey::Int(self.numeric)
        } else {
            ArrayKey::Interned(self)
        }
    }

    /// Materialises an owned `PhpString` with the same bytes (only for callers
    /// that require a mutable/owned `PhpString`).
    pub fn to_php_string(&self) -> PhpString {
        PhpString::from(self.bytes)
    }

    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(self.bytes).into_owned()
    }
}

impl PartialEq for InternedStr {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for InternedStr {}

impl PartialOrd for InternedStr {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InternedStr {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        if std::ptr::eq(self, other) {
            Ordering::Equal
        } else {
            self.bytes.cmp(other.bytes)
        }
    }
}

impl Hash for InternedStr {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bytes.hash(state);
    }
}

impl AsRef<[u8]> for InternedStr {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        self.bytes
    }
}

impl Deref for InternedStr {
    type Target = [u8];

    #[inline(always)]
    fn deref(&self) -> &[u8] {
        self.bytes
    }
}

impl fmt::Debug for InternedStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", String::from_utf8_lossy(self.bytes))
    }
}

impl fmt::Display for InternedStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(self.bytes))
    }
}

/// Borrowed view of either an interned or dynamic PHP string.
#[derive(Clone, Copy)]
pub enum StrRef<'a> {
    Interned(&'static InternedStr),
    Dynamic(&'a PhpString),
}

impl<'a> StrRef<'a> {
    #[inline(always)]
    pub fn as_bytes(self) -> &'a [u8] {
        match self {
            Self::Interned(s) => s.as_bytes(),
            Self::Dynamic(s) => s.as_bytes(),
        }
    }

    #[inline(always)]
    pub fn len(self) -> usize {
        self.as_bytes().len()
    }

    #[inline(always)]
    pub fn is_empty(self) -> bool {
        self.as_bytes().is_empty()
    }

    pub fn to_string_lossy(self) -> String {
        String::from_utf8_lossy(self.as_bytes()).into_owned()
    }

    pub fn to_php_string(self) -> PhpString {
        match self {
            Self::Interned(s) => s.to_php_string(),
            Self::Dynamic(s) => s.clone(),
        }
    }

    #[inline]
    pub fn to_value(self) -> Value {
        match self {
            Self::Interned(s) => Value::InternedStr(s),
            Self::Dynamic(s) => Value::String(s.clone()),
        }
    }

    #[inline]
    pub fn to_array_key(self) -> ArrayKey {
        match self {
            Self::Interned(s) => s.to_array_key(),
            Self::Dynamic(s) => ArrayKey::from_string(s.clone()),
        }
    }
}

impl Deref for StrRef<'_> {
    type Target = [u8];

    #[inline(always)]
    fn deref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl AsRef<[u8]> for StrRef<'_> {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl PartialEq for StrRef<'_> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Self::Interned(a), Self::Interned(b)) => std::ptr::eq(a, b),
            (a, b) => a.as_bytes() == b.as_bytes(),
        }
    }
}

impl Eq for StrRef<'_> {}

impl PartialEq<&[u8]> for StrRef<'_> {
    #[inline]
    fn eq(&self, other: &&[u8]) -> bool {
        self.as_bytes() == *other
    }
}

impl PartialEq<[u8]> for StrRef<'_> {
    #[inline]
    fn eq(&self, other: &[u8]) -> bool {
        self.as_bytes() == other
    }
}

impl PartialEq<&str> for StrRef<'_> {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl PartialOrd for StrRef<'_> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StrRef<'_> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        match (*self, *other) {
            (Self::Interned(a), Self::Interned(b)) if std::ptr::eq(a, b) => Ordering::Equal,
            (a, b) => a.as_bytes().cmp(b.as_bytes()),
        }
    }
}

impl fmt::Debug for StrRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", String::from_utf8_lossy(self.as_bytes()))
    }
}

impl fmt::Display for StrRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(self.as_bytes()))
    }
}

struct InternTable {
    entries: HashTable<&'static InternedStr>,
    total_bytes: usize,
}

impl InternTable {
    fn new() -> Self {
        Self {
            entries: HashTable::with_capacity(4_096),
            total_bytes: 0,
        }
    }

    /// Adds `bytes`, which hash to `hash` and are not in the table yet.
    fn intern_unchecked(&mut self, hash: u64, bytes: &[u8]) -> &'static InternedStr {
        let leaked_bytes: &'static [u8] = if bytes.is_empty() {
            b""
        } else {
            Box::leak(bytes.to_vec().into_boxed_slice())
        };
        let numeric = numeric_string_key(leaked_bytes);
        let truthy = !(leaked_bytes.is_empty() || leaked_bytes == b"0");
        let entry: &'static InternedStr = Box::leak(Box::new(InternedStr {
            hash,
            numeric: numeric.unwrap_or(0),
            bytes: leaked_bytes,
            is_numeric: numeric.is_some(),
            truthy,
        }));
        self.total_bytes += leaked_bytes.len();
        self.entries.insert_unique(hash, entry, |other| other.hash);
        entry
    }
}

fn interner() -> &'static Mutex<InternTable> {
    static TABLE: OnceLock<Mutex<InternTable>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(InternTable::new()))
}

thread_local! {
    /// The strings this thread has interned or found so far, as pointers
    /// into the process table. Entries are immortal and never change, so a
    /// thread that interns a name it saw before (a PDO column of the next
    /// query, a unit's literals in the next request) finds it here without
    /// taking the process lock that every worker thread shares.
    static SEEN: RefCell<HashTable<&'static InternedStr>> =
        RefCell::new(HashTable::with_capacity(1_024));
}

fn intern_non_empty(bytes: &[u8]) -> Option<&'static InternedStr> {
    let hash = hash_str_bytes(bytes);
    SEEN.with_borrow_mut(|seen| {
        if let Some(&existing) = seen.find(hash, |entry| entry.bytes == bytes) {
            return Some(existing);
        }
        let interned = intern_shared(hash, bytes)?;
        seen.insert_unique(hash, interned, |entry| entry.hash);
        Some(interned)
    })
}

/// Finds or adds `bytes` in the process table, under its lock.
#[cold]
#[inline(never)]
fn intern_shared(hash: u64, bytes: &[u8]) -> Option<&'static InternedStr> {
    let mut table = interner().lock().expect("interner poisoned");
    if let Some(&existing) = table.entries.find(hash, |entry| entry.bytes == bytes) {
        return Some(existing);
    }
    if table.entries.len() >= MAX_INTERNED_STRINGS
        || table.total_bytes.saturating_add(bytes.len()) > MAX_INTERNED_BYTES
    {
        return None;
    }
    Some(table.intern_unchecked(hash, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interning_deduplicates_and_classifies_numeric_keys() {
        let a = InternedStr::try_intern(b"status").unwrap();
        let b = InternedStr::try_intern(b"status").unwrap();
        assert!(std::ptr::eq(a, b));
        assert_eq!(a, b);
        assert_eq!(a.numeric_key(), None);
        assert!(a.is_truthy());

        let zero = InternedStr::try_intern(b"0").unwrap();
        assert_eq!(zero.numeric_key(), Some(0));
        assert!(!zero.is_truthy());
        assert_eq!(zero.to_array_key(), ArrayKey::Int(0));

        let leading = InternedStr::try_intern(b"05").unwrap();
        assert_eq!(leading.numeric_key(), None);
        assert!(leading.is_truthy());
        assert_eq!(leading.to_array_key(), ArrayKey::Interned(leading));

        let empty = InternedStr::try_intern(b"").unwrap();
        assert!(std::ptr::eq(empty, InternedStr::empty()));
        assert!(!empty.is_truthy());
    }

    #[test]
    fn threads_share_one_canonical_string_through_their_own_tables() {
        let here = InternedStr::try_intern(b"phera_intern_two_threads").unwrap();
        let there = std::thread::spawn(|| {
            let first = InternedStr::try_intern(b"phera_intern_two_threads").unwrap();
            let again = InternedStr::try_intern(b"phera_intern_two_threads").unwrap();
            assert!(std::ptr::eq(first, again));
            first as *const InternedStr as usize
        })
        .join()
        .unwrap();
        assert_eq!(here as *const InternedStr as usize, there);
        let only_there = std::thread::spawn(|| {
            InternedStr::try_intern(b"phera_intern_made_elsewhere").unwrap() as *const InternedStr
                as usize
        })
        .join()
        .unwrap();
        let seen_here = InternedStr::try_intern(b"phera_intern_made_elsewhere").unwrap();
        assert_eq!(seen_here as *const InternedStr as usize, only_there);
    }
}
