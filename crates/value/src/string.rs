use std::rc::Rc;
use std::{cell::Cell, fmt};

/// A PHP string: an arbitrary byte string (need not be UTF-8), cheap to clone
/// and copy-on-write, with amortised O(1) append for `.=`.
#[derive(Default, Clone)]
pub struct PhpString(Rc<SharedStr>);

#[derive(Default)]
struct SharedStr {
    hash: Cell<u64>,
    bytes: Vec<u8>,
}

impl Clone for SharedStr {
    fn clone(&self) -> Self {
        Self {
            hash: Cell::new(0),
            bytes: self.bytes.clone(),
        }
    }
}

impl PhpString {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends bytes, cloning the backing buffer only when shared.
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        let shared = Rc::make_mut(&mut self.0);
        shared.hash.set(0);
        shared.bytes.extend_from_slice(bytes);
    }

    /// Borrows mutable bytes only when this PHP string owns its buffer.
    /// A secure erase can preserve the bytes visible to other PHP values.
    pub fn as_unique_bytes_mut(&mut self) -> Option<&mut [u8]> {
        Rc::get_mut(&mut self.0).map(|shared| {
            shared.hash.set(0);
            shared.bytes.as_mut_slice()
        })
    }

    /// Mutate the bytes, preserving other PHP values that share this buffer.
    pub fn make_mut_bytes(&mut self) -> &mut [u8] {
        let shared = Rc::make_mut(&mut self.0);
        shared.hash.set(0);
        shared.bytes.as_mut_slice()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0.bytes
    }

    pub fn len(&self) -> usize {
        self.0.bytes.len()
    }

    /// Number of values sharing the backing storage, for `debug_zval_dump()`.
    pub fn storage_owners(&self) -> usize {
        Rc::strong_count(&self.0)
    }

    pub fn is_empty(&self) -> bool {
        self.0.bytes.is_empty()
    }

    /// The hash of this spelling under the process's array seed.
    #[inline]
    pub(crate) fn array_hash(&self) -> u64 {
        let hash = self.0.hash.get();
        if hash != 0 {
            return hash;
        }
        self.compute_array_hash()
    }

    #[cold]
    #[inline(never)]
    fn compute_array_hash(&self) -> u64 {
        let hash = crate::array::hash_str_bytes(self.as_bytes());
        self.0.hash.set(hash);
        hash
    }

    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(self.as_bytes()).into_owned()
    }
}

impl From<&str> for PhpString {
    fn from(s: &str) -> Self {
        Self::from(s.as_bytes())
    }
}

impl From<String> for PhpString {
    fn from(s: String) -> Self {
        Self::from(s.into_bytes())
    }
}

impl From<Vec<u8>> for PhpString {
    fn from(b: Vec<u8>) -> Self {
        Self(Rc::new(SharedStr {
            hash: Cell::new(0),
            bytes: b,
        }))
    }
}

impl From<&[u8]> for PhpString {
    fn from(b: &[u8]) -> Self {
        Self::from(b.to_vec())
    }
}

impl PartialEq for PhpString {
    fn eq(&self, other: &Self) -> bool {
        self.0.bytes == other.0.bytes
    }
}

impl Eq for PhpString {}

impl Ord for PhpString {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.bytes.cmp(&other.0.bytes)
    }
}

impl PartialOrd for PhpString {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::hash::Hash for PhpString {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.bytes.hash(state);
    }
}

impl std::ops::Deref for PhpString {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0.bytes
    }
}

impl fmt::Debug for PhpString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.to_string_lossy())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_on_write() {
        let a = PhpString::from("hello");
        let mut b = a.clone();
        b.push_bytes(b" world");
        assert_eq!(&*a, b"hello");
        assert_eq!(&*b, b"hello world");
    }

    #[test]
    fn append_amortised() {
        let mut s = PhpString::new();
        for i in 0..10_000u32 {
            s.push_bytes(i.to_string().as_bytes());
        }
        assert_eq!(s.len(), 38890);
    }
    #[test]
    fn cached_hash_invalidates_on_mutation() {
        let mut a = PhpString::from("alpha");
        let b = a.clone();
        let original = a.array_hash();
        assert_ne!(original, 0);
        assert_eq!(b.array_hash(), original);

        a.push_bytes(b"_beta");
        assert_eq!(b.array_hash(), original);
        assert_eq!(a.array_hash(), PhpString::from("alpha_beta").array_hash());

        let mut c = PhpString::from("alpha");
        let _ = c.array_hash();
        c.as_unique_bytes_mut().unwrap()[0] = b'A';
        assert_eq!(c, PhpString::from("Alpha"));
        assert_eq!(c.array_hash(), PhpString::from("Alpha").array_hash());

        let mut d = PhpString::from("gamma");
        let _ = d.array_hash();
        d.make_mut_bytes()[0] = b'G';
        assert_eq!(d, PhpString::from("Gamma"));
        assert_eq!(d.array_hash(), PhpString::from("Gamma").array_hash());
    }
}
