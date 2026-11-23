use std::fmt;
use std::rc::Rc;

use crate::intern::InternedStr;

#[derive(Clone)]
#[repr(u8)]
enum Repr {
    Interned(&'static InternedStr),
    Shared(Rc<Vec<u8>>),
}

/// A PHP byte string. Literal copies need no reference-count traffic.
/// Shared strings are copy-on-write.
#[derive(Clone)]
pub struct PhpString(Repr);

const _: () = {
    assert!(std::mem::size_of::<PhpString>() == 16);
    assert!(std::mem::size_of::<crate::Value>() == 16);
    assert!(std::mem::size_of::<Option<crate::Value>>() == 16);
    assert!(std::mem::size_of::<crate::ArrayKey>() == 16);
};

impl Default for PhpString {
    fn default() -> Self {
        Self::new()
    }
}

impl PhpString {
    /// Wrap a canonical literal such as `'name'` without allocating a buffer.
    #[inline]
    pub fn from_interned(value: &'static InternedStr) -> Self {
        Self(Repr::Interned(value))
    }

    /// Prepare a declaration or literal such as `'name'` while linking.
    pub fn intern_literal(bytes: &[u8]) -> Self {
        match InternedStr::try_intern(bytes) {
            Some(value) => Self::from_interned(value),
            None => Self::from(bytes),
        }
    }

    /// The process's canonical empty PHP string.
    pub fn empty() -> Self {
        Self::from_interned(InternedStr::empty())
    }

    pub fn new() -> Self {
        Self::from(Vec::new())
    }

    /// Append bytes, separating the shared buffer when necessary.
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        Rc::make_mut(self.shared_mut()).extend_from_slice(bytes);
    }

    /// Borrow mutable bytes if the buffer has no other handle.
    pub fn as_unique_bytes_mut(&mut self) -> Option<&mut [u8]> {
        Rc::get_mut(self.shared_mut()).map(Vec::as_mut_slice)
    }

    /// Mutate bytes without changing other PHP values.
    pub fn make_mut_bytes(&mut self) -> &mut [u8] {
        Rc::make_mut(self.shared_mut()).as_mut_slice()
    }

    fn shared_mut(&mut self) -> &mut Rc<Vec<u8>> {
        if let Repr::Interned(value) = self.0 {
            self.0 = Repr::Shared(Rc::new(value.as_bytes().to_vec()));
        }
        match &mut self.0 {
            Repr::Shared(storage) => storage,
            Repr::Interned(_) => unreachable!("an immutable string was materialized above"),
        }
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        match &self.0 {
            Repr::Interned(value) => value.as_bytes(),
            Repr::Shared(storage) => storage.as_slice(),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.as_bytes().len()
    }

    /// Whether `debug_zval_dump()` should describe this value as a literal.
    pub fn is_literal(&self) -> bool {
        matches!(self.0, Repr::Interned(_))
    }

    /// Whether a PHP string is truthy, including the special case `'0'`.
    #[inline]
    pub(crate) fn is_truthy(&self) -> bool {
        match &self.0 {
            Repr::Interned(value) => value.is_truthy(),
            Repr::Shared(bytes) => !(bytes.is_empty() || bytes.as_slice() == b"0"),
        }
    }

    /// Number of handles sharing writable backing storage.
    pub fn storage_owners(&self) -> usize {
        match &self.0 {
            Repr::Interned(_) => 1,
            Repr::Shared(storage) => Rc::strong_count(storage),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether two handles point to the same bytes.
    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Repr::Interned(left), Repr::Interned(right)) => std::ptr::eq(*left, *right),
            (Repr::Shared(left), Repr::Shared(right)) => Rc::ptr_eq(left, right),
            _ => false,
        }
    }

    /// The hash of these bytes under the process-wide array seed.
    #[inline]
    pub(crate) fn array_hash(&self) -> u64 {
        match &self.0 {
            Repr::Interned(value) => value.array_hash(),
            Repr::Shared(bytes) => crate::array::hash_str_bytes(bytes),
        }
    }

    /// The canonical integer array key for a spelling such as `'-8'`.
    #[inline]
    pub(crate) fn numeric_key(&self) -> Option<i64> {
        match &self.0 {
            Repr::Interned(value) => value.numeric_key(),
            Repr::Shared(bytes) => crate::array::numeric_string_key(bytes),
        }
    }

    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(self.as_bytes()).into_owned()
    }
}

impl From<&str> for PhpString {
    #[inline]
    fn from(s: &str) -> Self {
        Self::from(s.as_bytes())
    }
}

impl From<String> for PhpString {
    #[inline]
    fn from(s: String) -> Self {
        Self::from(s.into_bytes())
    }
}

impl From<Vec<u8>> for PhpString {
    #[inline]
    fn from(bytes: Vec<u8>) -> Self {
        Self(Repr::Shared(Rc::new(bytes)))
    }
}

impl From<&[u8]> for PhpString {
    #[inline]
    fn from(bytes: &[u8]) -> Self {
        Self::from(bytes.to_vec())
    }
}

impl PartialEq for PhpString {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Repr::Interned(left), Repr::Interned(right)) => std::ptr::eq(*left, *right),
            (Repr::Shared(left), Repr::Shared(right)) if Rc::ptr_eq(left, right) => true,
            _ => self.as_bytes() == other.as_bytes(),
        }
    }
}
impl Eq for PhpString {}

impl Ord for PhpString {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.ptr_eq(other) {
            return std::cmp::Ordering::Equal;
        }
        self.as_bytes().cmp(other.as_bytes())
    }
}

impl PartialOrd for PhpString {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::hash::Hash for PhpString {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state);
    }
}

impl std::ops::Deref for PhpString {
    type Target = [u8];
    #[inline]
    fn deref(&self) -> &[u8] {
        self.as_bytes()
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
    fn a_literal_mutates_without_touching_the_canonical_bytes() {
        let original = PhpString::intern_literal(b"interned-string-test");
        let mut copy = original.clone();
        assert!(copy.ptr_eq(&original));
        assert_eq!(original, PhpString::from("interned-string-test"));
        copy.push_bytes(b"-suffix");
        assert!(!copy.is_literal());
        assert_eq!(
            copy.array_hash(),
            PhpString::from("interned-string-test-suffix").array_hash()
        );
        assert!(original.is_literal());
        assert_eq!(&*original, b"interned-string-test");

        let mut unique = original.clone();
        unique.as_unique_bytes_mut().unwrap()[0] = b'I';
        assert_eq!(&*unique, b"Interned-string-test");
        let mut writable = original.clone();
        assert!(writable.is_literal());
        writable.make_mut_bytes()[0] = b'I';
        assert_eq!(unique, writable);
        assert!(!writable.is_literal());
        assert_eq!(&*original, b"interned-string-test");
    }

    #[test]
    fn append_amortised() {
        let mut s = PhpString::new();
        for i in 0..10_000u32 {
            s.push_bytes(i.to_string().as_bytes());
        }
        assert_eq!(s.len(), 38890);
    }
}
