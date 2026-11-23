use std::rc::Rc;
use std::{cell::Cell, fmt};

use crate::intern::InternedStr;

/// Maximum length of a string stored inline inside `SharedStr`.
///
/// `RcInner<SharedStr>` is `16 (strong + weak) + 8 (hash) + 40 (Bytes) = 64`
/// bytes: one cache line and a power-of-two mimalloc size class (which avoids
/// mimalloc's aligned-overalloc fallback on 48-byte allocations) while holding
/// strings up to 38 bytes without a second heap allocation.
const INLINE_CAP: usize = 38;

enum Bytes {
    Inline { len: u8, data: [u8; INLINE_CAP] },
    Heap(Vec<u8>),
}

impl Bytes {
    #[inline]
    fn from_slice(bytes: &[u8]) -> Self {
        if bytes.len() <= INLINE_CAP {
            let mut data = [0u8; INLINE_CAP];
            data[..bytes.len()].copy_from_slice(bytes);
            Self::Inline {
                len: bytes.len() as u8,
                data,
            }
        } else {
            Self::Heap(bytes.to_vec())
        }
    }

    #[inline]
    fn from_pair(left: &[u8], right: &[u8]) -> Self {
        let total = left.len() + right.len();
        if total <= INLINE_CAP {
            let mut data = [0u8; INLINE_CAP];
            data[..left.len()].copy_from_slice(left);
            data[left.len()..total].copy_from_slice(right);
            Self::Inline {
                len: total as u8,
                data,
            }
        } else {
            let mut out = Vec::with_capacity(total);
            out.extend_from_slice(left);
            out.extend_from_slice(right);
            Self::Heap(out)
        }
    }

    #[inline]
    fn from_vec(bytes: Vec<u8>) -> Self {
        if bytes.capacity() == 0 {
            Self::Inline {
                len: 0,
                data: [0u8; INLINE_CAP],
            }
        } else {
            Self::Heap(bytes)
        }
    }

    #[inline]
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Inline { len, data } => &data[..usize::from(*len)],
            Self::Heap(bytes) => bytes.as_slice(),
        }
    }

    #[inline]
    fn as_mut_slice(&mut self) -> &mut [u8] {
        match self {
            Self::Inline { len, data } => &mut data[..usize::from(*len)],
            Self::Heap(bytes) => bytes.as_mut_slice(),
        }
    }

    #[inline]
    fn len(&self) -> usize {
        match self {
            Self::Inline { len, .. } => usize::from(*len),
            Self::Heap(bytes) => bytes.len(),
        }
    }

    fn push_bytes(&mut self, extra: &[u8]) {
        match self {
            Self::Inline { len, data } => {
                let current = usize::from(*len);
                let total = current.checked_add(extra.len());
                if let Some(total) = total
                    && total <= INLINE_CAP
                {
                    data[current..total].copy_from_slice(extra);
                    *len = total as u8;
                } else {
                    let mut out = Vec::with_capacity(current.saturating_add(extra.len()));
                    out.extend_from_slice(&data[..current]);
                    out.extend_from_slice(extra);
                    *self = Self::Heap(out);
                }
            }
            Self::Heap(bytes) => bytes.extend_from_slice(extra),
        }
    }
}

impl Clone for Bytes {
    fn clone(&self) -> Self {
        match self {
            Self::Inline { len, data } => Self::Inline {
                len: *len,
                data: *data,
            },
            Self::Heap(bytes) => Self::Heap(bytes.clone()),
        }
    }
}

struct SharedStr {
    hash: Cell<u64>,
    bytes: Bytes,
}

const _: () = assert!(std::mem::size_of::<SharedStr>() == 48);

impl Clone for SharedStr {
    fn clone(&self) -> Self {
        // Copy-on-write separates this buffer before mutation.
        Self {
            hash: Cell::new(0),
            bytes: self.bytes.clone(),
        }
    }
}

#[derive(Clone)]
enum Repr {
    Interned(&'static InternedStr),
    Shared {
        storage: Rc<SharedStr>,
        // A compiled literal keeps its diagnostic if the interner is full.
        literal: bool,
    },
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
    fn from_shared_bytes(bytes: Bytes, literal: bool) -> Self {
        Self(Repr::Shared {
            storage: Rc::new(SharedStr {
                hash: Cell::new(0),
                bytes,
            }),
            literal,
        })
    }

    /// Wrap a canonical literal such as `'name'` without allocating a buffer.
    #[inline]
    pub fn from_interned(value: &'static InternedStr) -> Self {
        Self(Repr::Interned(value))
    }

    /// Materialize a compiled literal, using shared bytes if it was not interned.
    #[inline]
    pub fn from_interned_or_bytes(value: Option<&'static InternedStr>, bytes: &[u8]) -> Self {
        match value {
            Some(value) => Self::from_interned(value),
            None => Self::from_shared_bytes(Bytes::from_slice(bytes), true),
        }
    }

    /// Look up a trusted name such as `'stdClass'` during linking.
    pub fn intern_literal(bytes: &[u8]) -> Self {
        Self::from_interned_or_bytes(InternedStr::try_intern(bytes), bytes)
    }

    pub fn new() -> Self {
        Self::from(Vec::new())
    }

    /// Copy two string operands into their final storage for `$left . $right`.
    pub(crate) fn from_pair(left: &[u8], right: &[u8]) -> Self {
        Self::from_shared_bytes(Bytes::from_pair(left, right), false)
    }

    /// Append bytes, separating the shared buffer when necessary.
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let (storage, literal) = self.shared_mut();
        let storage = Rc::make_mut(storage);
        *literal = false;
        storage.hash.set(0);
        storage.bytes.push_bytes(bytes);
    }

    /// Borrow mutable bytes only when the handle owns its buffer. The
    /// canonical bytes of an interned string are left intact.
    pub fn as_unique_bytes_mut(&mut self) -> Option<&mut [u8]> {
        let (storage, literal) = self.shared_mut();
        let storage = Rc::get_mut(storage)?;
        *literal = false;
        storage.hash.set(0);
        Some(storage.bytes.as_mut_slice())
    }

    /// Mutate the bytes without changing other PHP values that share them.
    pub fn make_mut_bytes(&mut self) -> &mut [u8] {
        let (storage, literal) = self.shared_mut();
        let storage = Rc::make_mut(storage);
        *literal = false;
        storage.hash.set(0);
        storage.bytes.as_mut_slice()
    }

    fn shared_mut(&mut self) -> (&mut Rc<SharedStr>, &mut bool) {
        if let Repr::Interned(value) = self.0 {
            *self = Self::from(value.bytes);
        }
        match &mut self.0 {
            Repr::Shared { storage, literal } => (storage, literal),
            Repr::Interned(_) => unreachable!("an immutable string was materialized above"),
        }
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        match &self.0 {
            Repr::Interned(value) => value.bytes,
            Repr::Shared { storage, .. } => storage.bytes.as_slice(),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        match &self.0 {
            Repr::Interned(value) => value.bytes.len(),
            Repr::Shared { storage, .. } => storage.bytes.len(),
        }
    }

    /// Whether `debug_zval_dump()` should describe this value as a literal.
    pub fn is_literal(&self) -> bool {
        match &self.0 {
            Repr::Interned(_) => true,
            Repr::Shared { literal, .. } => *literal,
        }
    }

    /// Number of handles sharing writable backing storage.
    pub fn storage_owners(&self) -> usize {
        match &self.0 {
            Repr::Interned(_) => 1,
            Repr::Shared { storage, .. } => Rc::strong_count(storage),
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
            (Repr::Shared { storage: left, .. }, Repr::Shared { storage: right, .. }) => {
                Rc::ptr_eq(left, right)
            }
            _ => false,
        }
    }

    /// The hash of these bytes under the process-wide array seed.
    #[inline]
    pub(crate) fn array_hash(&self) -> u64 {
        let shared = match &self.0 {
            Repr::Interned(value) => return value.hash,
            Repr::Shared { storage, .. } => storage,
        };
        let cached = shared.hash.get();
        if cached != 0 {
            return cached;
        }
        Self::compute_array_hash(shared)
    }

    #[cold]
    #[inline(never)]
    fn compute_array_hash(shared: &SharedStr) -> u64 {
        let hash = crate::array::hash_str_bytes(shared.bytes.as_slice());
        shared.hash.set(hash);
        hash
    }

    /// The canonical integer array key for a spelling such as `'-8'`.
    #[inline]
    pub(crate) fn numeric_key(&self) -> Option<i64> {
        match &self.0 {
            Repr::Interned(value) => value.numeric_key,
            Repr::Shared { storage, .. } => {
                crate::array::numeric_string_key(storage.bytes.as_slice())
            }
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
        Self::from_shared_bytes(Bytes::from_vec(bytes), false)
    }
}

impl From<&[u8]> for PhpString {
    #[inline]
    fn from(bytes: &[u8]) -> Self {
        Self::from_shared_bytes(Bytes::from_slice(bytes), false)
    }
}

impl PartialEq for PhpString {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Repr::Interned(left), Repr::Interned(right)) => std::ptr::eq(*left, *right),
            (Repr::Shared { storage: left, .. }, Repr::Shared { storage: right, .. })
                if Rc::ptr_eq(left, right) =>
            {
                true
            }
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
        let mut writable = PhpString::from_interned_or_bytes(None, b"interned-string-test");
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

    #[test]
    fn cached_hash_invalidates_on_mutation() {
        let mut a = PhpString::from("alpha");
        let b = a.clone();
        let original = a.array_hash();
        assert_ne!(original, 0);
        assert_eq!(b.array_hash(), original);
        assert!(a.ptr_eq(&b));

        a.push_bytes(b"_beta");
        assert!(!a.ptr_eq(&b));
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
    #[test]
    fn inline_and_heap_promotion_boundary() {
        let mut s = PhpString::from("a".repeat(INLINE_CAP).as_str());
        assert_eq!(s.len(), INLINE_CAP);
        assert!(
            matches!(&s.0, Repr::Shared { storage, .. } if matches!(storage.bytes, Bytes::Inline { .. }))
        );
        let copy = s.clone();

        s.push_bytes(b"!");
        assert_eq!(s.len(), INLINE_CAP + 1);
        assert!(
            matches!(&s.0, Repr::Shared { storage, .. } if matches!(storage.bytes, Bytes::Heap(_)))
        );
        assert_eq!(copy.len(), INLINE_CAP);
        assert!(
            matches!(&copy.0, Repr::Shared { storage, .. } if matches!(storage.bytes, Bytes::Inline { .. }))
        );
        assert_eq!(&*s, format!("{}!", "a".repeat(INLINE_CAP)).as_bytes());

        let pair_inline = PhpString::from_pair(b"route:", b"hello");
        assert!(
            matches!(&pair_inline.0, Repr::Shared { storage, .. } if matches!(storage.bytes, Bytes::Inline { .. }))
        );
        assert_eq!(&*pair_inline, b"route:hello");

        let pair_heap = PhpString::from_pair(&[b'x'; 20], &[b'y'; 20]);
        assert!(
            matches!(&pair_heap.0, Repr::Shared { storage, .. } if matches!(storage.bytes, Bytes::Heap(_)))
        );
        assert_eq!(pair_heap.len(), 40);
    }
}
