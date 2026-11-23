//! Canonical bytes for trusted compiled literals and linked names.
//!
//! Callers may intern source supplied as application code, shipped code and
//! declarations registered by extensions. PHP values, evaluated code and
//! request-selected include sources must use shared strings. The entry,
//! byte and length limits bound storage for the lifetime of the process.

use std::sync::{Mutex, OnceLock, PoisonError};

use hashbrown::HashTable;

const MAX_ENTRIES: usize = 131_072;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_LENGTH: usize = 4_096;

/// The bytes and array-key metadata of a trusted literal such as `'name'`.
#[derive(Debug, PartialEq, Eq)]
pub struct InternedStr {
    pub(crate) bytes: &'static [u8],
    pub(crate) hash: u64,
    pub(crate) numeric_key: Option<i64>,
}

impl InternedStr {
    /// Canonical bytes, which remain valid for the lifetime of the process.
    pub fn as_bytes(&self) -> &'static [u8] {
        self.bytes
    }

    /// Finds or records trusted bytes while compiling or linking. A full
    /// table or a long literal leaves the caller to use a shared string.
    pub fn try_intern(bytes: &[u8]) -> Option<&'static Self> {
        if bytes.len() > MAX_LENGTH {
            return None;
        }
        static TABLE: OnceLock<Mutex<InternTable>> = OnceLock::new();
        let hash = crate::array::hash_str_bytes(bytes);
        TABLE
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .intern(bytes, hash, MAX_ENTRIES, MAX_BYTES)
    }
}

#[derive(Default)]
struct InternTable {
    entries: HashTable<&'static InternedStr>,
    bytes: usize,
}

impl InternTable {
    fn intern(
        &mut self,
        bytes: &[u8],
        hash: u64,
        max_entries: usize,
        max_bytes: usize,
    ) -> Option<&'static InternedStr> {
        if let Some(&entry) = self.entries.find(hash, |entry| entry.bytes == bytes) {
            return Some(entry);
        }
        if self.entries.len() >= max_entries || self.bytes + bytes.len() > max_bytes {
            return None;
        }
        let entry = Box::leak(Box::new(InternedStr {
            bytes: Box::leak(Box::<[u8]>::from(bytes)),
            hash,
            numeric_key: crate::array::numeric_string_key(bytes),
        }));
        self.entries.insert_unique(hash, entry, |other| other.hash);
        self.bytes += bytes.len();
        Some(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ArrayKey, PhpString};

    #[test]
    fn full_interner_reuses_entries_and_falls_back_to_shared() {
        let mut table = InternTable::default();
        let intern = |table: &mut InternTable, bytes: &[u8]| {
            table.intern(bytes, crate::array::hash_str_bytes(bytes), 2, 4)
        };
        let one = intern(&mut table, b"17").unwrap();
        let two = intern(&mut table, b"ab").unwrap();
        assert!(std::ptr::eq(one, intern(&mut table, b"17").unwrap()));
        assert!(intern(&mut table, b"18").is_none());

        let mut byte_limited = InternTable::default();
        assert!(byte_limited.intern(b"123", 1, 4, 2).is_none());
        let fallback = PhpString::from_interned_or_bytes(intern(&mut table, b"18"), b"18");
        assert!(fallback.is_literal());
        assert_eq!(ArrayKey::from_string(fallback), ArrayKey::Int(18));
        assert_eq!(
            PhpString::from_interned(two).array_hash(),
            PhpString::from("ab").array_hash()
        );
    }

    #[test]
    fn entry_limit_bounds_the_number_of_immortal_strings() {
        // Use a separate table so the test does not exhaust the process
        // table used by other tests.
        let mut table = InternTable::default();
        for index in 0..MAX_ENTRIES {
            let bytes = (index as u64).to_le_bytes();
            let hash = crate::array::hash_str_bytes(&bytes);
            assert!(table.intern(&bytes, hash, MAX_ENTRIES, MAX_BYTES).is_some());
        }
        let bytes = b"over the entry limit";
        let overflow = table.intern(
            bytes,
            crate::array::hash_str_bytes(bytes),
            MAX_ENTRIES,
            MAX_BYTES,
        );
        assert!(overflow.is_none());
        assert_eq!(
            PhpString::from_interned_or_bytes(overflow, bytes),
            PhpString::from(bytes.as_slice())
        );
    }

    #[test]
    fn concurrent_linkers_find_one_canonical_string() {
        let first = InternedStr::try_intern(b"concurrent-linker-test").unwrap();
        std::thread::scope(|scope| {
            let threads: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| InternedStr::try_intern(b"concurrent-linker-test").unwrap())
                })
                .collect();
            for thread in threads {
                assert!(std::ptr::eq(first, thread.join().unwrap()));
            }
        });
    }
}
