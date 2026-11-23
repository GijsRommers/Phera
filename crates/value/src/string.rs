use std::cell::Cell;
use std::fmt;
use std::rc::Rc;

/// Maximum length of a string stored inline inside `PhpStringInner`.
///
/// `RcInner<PhpStringInner>` is `16 (strong + weak) + 8 (hash) + 40 (Bytes) = 64`
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

struct PhpStringInner {
    /// Cached hash under `crate::array::hash_str_bytes`, with `0` meaning
    /// uncomputed. Computed hashes always have bit 0 set (`hash | 1`) so `0`
    /// stays reserved as the sentinel while keeping 63 bits of entropy
    /// (including the top bits used by SwissTable's control byte).
    hash: Cell<u64>,
    bytes: Bytes,
}

const _: () = assert!(std::mem::size_of::<PhpStringInner>() == 48);

impl Clone for PhpStringInner {
    fn clone(&self) -> Self {
        // Only invoked by `Rc::make_mut` right before mutating the bytes, so
        // leave the separated copy's hash uncomputed.
        Self {
            hash: Cell::new(0),
            bytes: self.bytes.clone(),
        }
    }
}

/// A PHP string: an arbitrary byte string (need not be UTF-8), cheap to clone
/// and copy-on-write, with amortised O(1) append for `.=`.
#[derive(Clone)]
pub struct PhpString(Rc<PhpStringInner>);

const _: () = assert!(std::mem::size_of::<PhpString>() == 8);

impl Default for PhpString {
    fn default() -> Self {
        Self::new()
    }
}

impl PhpString {
    #[inline]
    fn from_bytes_storage(bytes: Bytes) -> Self {
        PhpString(Rc::new(PhpStringInner {
            hash: Cell::new(0),
            bytes,
        }))
    }

    pub fn new() -> Self {
        Self::from_bytes_storage(Bytes::Inline {
            len: 0,
            data: [0u8; INLINE_CAP],
        })
    }

    /// Concatenates two byte slices directly, avoiding an intermediate `Vec`
    /// allocation when the result fits inline.
    #[inline]
    pub fn from_pair(left: &[u8], right: &[u8]) -> Self {
        Self::from_bytes_storage(Bytes::from_pair(left, right))
    }

    /// Appends bytes, cloning the backing buffer only when shared.
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        let inner = Rc::make_mut(&mut self.0);
        inner.hash.set(0);
        inner.bytes.push_bytes(bytes);
    }

    /// Borrows mutable bytes only when this PHP string owns its buffer.
    /// A secure erase can preserve the bytes visible to other PHP values.
    pub fn as_unique_bytes_mut(&mut self) -> Option<&mut [u8]> {
        Rc::get_mut(&mut self.0).map(|inner| {
            inner.hash.set(0);
            inner.bytes.as_mut_slice()
        })
    }

    /// Mutate the bytes, preserving other PHP values that share this buffer.
    pub fn make_mut_bytes(&mut self) -> &mut [u8] {
        let inner = Rc::make_mut(&mut self.0);
        inner.hash.set(0);
        inner.bytes.as_mut_slice()
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.bytes.as_slice()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.0.bytes.len()
    }

    /// Number of values sharing the backing storage, for `debug_zval_dump()`.
    pub fn storage_owners(&self) -> usize {
        Rc::strong_count(&self.0)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether two string handles share the same backing allocation.
    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    /// Hash of these bytes under the process-wide array hash seed, computed
    /// once and cached on the backing allocation.
    #[inline]
    pub(crate) fn array_hash(&self) -> u64 {
        let cached = self.0.hash.get();
        if cached != 0 {
            return cached;
        }
        self.compute_array_hash()
    }

    #[cold]
    #[inline(never)]
    fn compute_array_hash(&self) -> u64 {
        let hash = crate::array::hash_str_bytes(self.as_bytes()) | 1;
        self.0.hash.set(hash);
        hash
    }

    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(self.as_bytes()).into_owned()
    }
}

impl From<&str> for PhpString {
    #[inline]
    fn from(s: &str) -> Self {
        Self::from_bytes_storage(Bytes::from_slice(s.as_bytes()))
    }
}

impl From<String> for PhpString {
    #[inline]
    fn from(s: String) -> Self {
        Self::from_bytes_storage(Bytes::from_vec(s.into_bytes()))
    }
}

impl From<Vec<u8>> for PhpString {
    #[inline]
    fn from(b: Vec<u8>) -> Self {
        Self::from_bytes_storage(Bytes::from_vec(b))
    }
}

impl From<&[u8]> for PhpString {
    #[inline]
    fn from(b: &[u8]) -> Self {
        Self::from_bytes_storage(Bytes::from_slice(b))
    }
}

impl PartialEq for PhpString {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.as_bytes() == other.as_bytes()
    }
}

impl Eq for PhpString {}

impl Ord for PhpString {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if Rc::ptr_eq(&self.0, &other.0) {
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
        assert!(matches!(s.0.bytes, Bytes::Inline { .. }));
        let copy = s.clone();

        s.push_bytes(b"!");
        assert_eq!(s.len(), INLINE_CAP + 1);
        assert!(matches!(s.0.bytes, Bytes::Heap(_)));
        assert_eq!(copy.len(), INLINE_CAP);
        assert!(matches!(copy.0.bytes, Bytes::Inline { .. }));
        assert_eq!(&*s, format!("{}!", "a".repeat(INLINE_CAP)).as_bytes());

        let pair_inline = PhpString::from_pair(b"route:", b"hello");
        assert!(matches!(pair_inline.0.bytes, Bytes::Inline { .. }));
        assert_eq!(&*pair_inline, b"route:hello");

        let pair_heap = PhpString::from_pair(&[b'x'; 20], &[b'y'; 20]);
        assert!(matches!(pair_heap.0.bytes, Bytes::Heap(_)));
        assert_eq!(pair_heap.len(), 40);
    }
}
