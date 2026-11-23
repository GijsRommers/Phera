//! Immortal deduplicated PHP strings with pre-computed array hash and
//! numeric-key classification.
//!
//! Host-selected source, preloaded and shipped PHP, and extension declarations
//! may intern literals or declared names. A value supplied by a request, a
//! request-selected source revision, a stream include or eval must not enter
//! this table. Only fixed implementation spellings may be initialized on their
//! first runtime use, and their pointers are retained afterwards.
//!
//! The entry, payload and per-string limits bound lifetime storage. Every
//! `&'static InternedStr` comes from the single deduplicating table or is the canonical empty string. Pointer
//! equality (`std::ptr::eq`) is exact byte equality. Cloning and dropping an
//! interned `PhpString` does no reference counting, and
//! array lookups reuse the pre-computed hash and numeric-key classification.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::{Mutex, OnceLock};

use hashbrown::HashTable;

use crate::array::{hash_str_bytes, numeric_string_key};

/// Maximum number of unique strings held in the process-wide interner.
const MAX_INTERNED_STRINGS: usize = 131_072;
/// Maximum total byte payload held in the process-wide interner (16 MiB).
const MAX_INTERNED_BYTES: usize = 16 * 1024 * 1024;
/// Maximum length of a single string eligible for interning.
pub(crate) const MAX_SINGLE_INTERN_LEN: usize = 4_096;

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
        static EMPTY: OnceLock<InternedStr> = OnceLock::new();
        EMPTY.get_or_init(|| Self {
            hash: hash_str_bytes(b""),
            numeric: 0,
            bytes: b"",
            is_numeric: false,
            truthy: false,
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

    fn intern(&mut self, bytes: &[u8], hash: u64) -> Option<&'static InternedStr> {
        if let Some(&existing) = self.entries.find(hash, |entry| entry.bytes == bytes) {
            return Some(existing);
        }
        if self.entries.len() >= MAX_INTERNED_STRINGS
            || self.total_bytes.saturating_add(bytes.len()) > MAX_INTERNED_BYTES
        {
            return None;
        }
        let leaked_bytes = Box::leak(Box::<[u8]>::from(bytes));
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
        Some(entry)
    }
}

fn interner() -> &'static Mutex<InternTable> {
    static TABLE: OnceLock<Mutex<InternTable>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(InternTable::new()))
}

fn intern_non_empty(bytes: &[u8]) -> Option<&'static InternedStr> {
    let hash = hash_str_bytes(bytes);
    interner()
        .lock()
        .expect("interner poisoned")
        .intern(bytes, hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_table_retains_old_entries_and_declines_new_ones() {
        let mut table = InternTable::new();
        for index in 0..MAX_INTERNED_STRINGS as u64 {
            let bytes = index.to_le_bytes();
            assert!(table.intern(&bytes, hash_str_bytes(&bytes)).is_some());
        }
        let first = 0u64.to_le_bytes();
        assert!(table.intern(&first, hash_str_bytes(&first)).is_some());
        let extra = b"literal after the entry limit";
        let entry = table.intern(extra, hash_str_bytes(extra));
        assert!(entry.is_none());
        let fallback = crate::PhpString::from_interned_or_bytes(entry, extra);
        assert_eq!(fallback.as_bytes(), extra);
    }

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

        let leading = InternedStr::try_intern(b"05").unwrap();
        assert_eq!(leading.numeric_key(), None);
        assert!(leading.is_truthy());

        let empty = InternedStr::try_intern(b"").unwrap();
        assert!(std::ptr::eq(empty, InternedStr::empty()));
        assert!(!empty.is_truthy());
    }
}
