use std::fmt;
use std::rc::Rc;

/// A PHP string: an arbitrary byte string (need not be UTF-8), cheap to clone
/// and copy-on-write, with amortised O(1) append for `.=`.
///
/// A string of at most [`INLINE_CAPACITY`] bytes is kept in the value itself
/// (array keys, `dechex()` results, short concatenations, JSON fields): it
/// costs no allocation, and a copy is 16 bytes. A longer one shares one
/// buffer between its copies until one of them changes it.
///
/// ```text
/// "ff"            Inline { len: 2, bytes: "ff\0\0..." }
/// "a longer text" Heap(Rc<Vec<u8>>), shared by every copy
/// ```
#[derive(Clone)]
pub struct PhpString(Repr);

/// The most bytes a string keeps in the value itself.
pub const INLINE_CAPACITY: usize = 14;

#[derive(Clone)]
enum Repr {
    Inline(Inline),
    Heap(Rc<Vec<u8>>),
}

/// Up to [`INLINE_CAPACITY`] bytes and their length. The length is an enum
/// of its 15 possible values, so the byte that holds it has 241 values a
/// length never takes: `Repr`, `Value` and `ArrayKey` keep their variant
/// there, and all stay 16 bytes.
#[derive(Clone, Copy)]
struct Inline {
    len: InlineLen,
    bytes: [u8; INLINE_CAPACITY],
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum InlineLen {
    L0 = 0,
    L1,
    L2,
    L3,
    L4,
    L5,
    L6,
    L7,
    L8,
    L9,
    L10,
    L11,
    L12,
    L13,
    L14,
}

const _: () = assert!(std::mem::size_of::<PhpString>() == 16);
const _: () = assert!(std::mem::size_of::<Option<PhpString>>() == 16);

impl InlineLen {
    const ALL: [InlineLen; INLINE_CAPACITY + 1] = [
        Self::L0,
        Self::L1,
        Self::L2,
        Self::L3,
        Self::L4,
        Self::L5,
        Self::L6,
        Self::L7,
        Self::L8,
        Self::L9,
        Self::L10,
        Self::L11,
        Self::L12,
        Self::L13,
        Self::L14,
    ];
}

impl Inline {
    /// `None` when `bytes` is longer than [`INLINE_CAPACITY`].
    #[inline]
    fn new(bytes: &[u8]) -> Option<Self> {
        let len = *InlineLen::ALL.get(bytes.len())?;
        let mut inline = Inline {
            len,
            bytes: [0; INLINE_CAPACITY],
        };
        inline.bytes[..bytes.len()].copy_from_slice(bytes);
        Some(inline)
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.len as usize
    }

    #[inline(always)]
    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len()]
    }

    #[inline(always)]
    fn as_bytes_mut(&mut self) -> &mut [u8] {
        let len = self.len();
        &mut self.bytes[..len]
    }
}

impl Default for PhpString {
    fn default() -> Self {
        Self::new()
    }
}

impl PhpString {
    /// The empty string, which allocates nothing.
    #[inline]
    pub fn new() -> Self {
        PhpString(Repr::Inline(Inline {
            len: InlineLen::L0,
            bytes: [0; INLINE_CAPACITY],
        }))
    }

    /// Appends bytes, cloning the backing buffer only when shared. A short
    /// string that outgrows the value moves into a buffer with room for
    /// more appends.
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        match &mut self.0 {
            Repr::Inline(inline) => {
                let len = inline.len();
                let total = len + bytes.len();
                if let Some(&new_len) = InlineLen::ALL.get(total) {
                    inline.bytes[len..total].copy_from_slice(bytes);
                    inline.len = new_len;
                } else {
                    let mut buffer = Vec::with_capacity(total.max(2 * INLINE_CAPACITY));
                    buffer.extend_from_slice(inline.as_bytes());
                    buffer.extend_from_slice(bytes);
                    self.0 = Repr::Heap(Rc::new(buffer));
                }
            }
            Repr::Heap(buffer) => Rc::make_mut(buffer).extend_from_slice(bytes),
        }
    }

    /// Borrows mutable bytes only when this PHP string owns its buffer.
    /// A secure erase can preserve the bytes visible to other PHP values.
    pub fn as_unique_bytes_mut(&mut self) -> Option<&mut [u8]> {
        match &mut self.0 {
            Repr::Inline(inline) => Some(inline.as_bytes_mut()),
            Repr::Heap(buffer) => Rc::get_mut(buffer).map(Vec::as_mut_slice),
        }
    }

    /// Mutate the bytes, preserving other PHP values that share this buffer.
    pub fn make_mut_bytes(&mut self) -> &mut [u8] {
        match &mut self.0 {
            Repr::Inline(inline) => inline.as_bytes_mut(),
            Repr::Heap(buffer) => Rc::make_mut(buffer).as_mut_slice(),
        }
    }

    /// The bytes as an owned buffer: moved out when this is the only
    /// handle on them, copied otherwise.
    pub fn into_bytes(self) -> Vec<u8> {
        match self.0 {
            Repr::Inline(inline) => inline.as_bytes().to_vec(),
            Repr::Heap(buffer) => {
                Rc::try_unwrap(buffer).unwrap_or_else(|shared| shared.as_ref().clone())
            }
        }
    }

    #[inline(always)]
    pub fn as_bytes(&self) -> &[u8] {
        match &self.0 {
            Repr::Inline(inline) => inline.as_bytes(),
            Repr::Heap(buffer) => buffer,
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        match &self.0 {
            Repr::Inline(inline) => inline.len(),
            Repr::Heap(buffer) => buffer.len(),
        }
    }

    /// Number of values sharing the backing storage, for `debug_zval_dump()`.
    /// A string kept in the value has no storage to share.
    pub fn storage_owners(&self) -> usize {
        match &self.0 {
            Repr::Inline(_) => 1,
            Repr::Heap(buffer) => Rc::strong_count(buffer),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(self.as_bytes()).into_owned()
    }
}

impl From<&str> for PhpString {
    fn from(s: &str) -> Self {
        PhpString::from(s.as_bytes())
    }
}

impl From<String> for PhpString {
    fn from(s: String) -> Self {
        PhpString::from(s.into_bytes())
    }
}

impl From<Vec<u8>> for PhpString {
    #[inline]
    fn from(b: Vec<u8>) -> Self {
        match Inline::new(&b) {
            Some(inline) => PhpString(Repr::Inline(inline)),
            None => PhpString(Repr::Heap(Rc::new(b))),
        }
    }
}

impl From<&[u8]> for PhpString {
    #[inline]
    fn from(b: &[u8]) -> Self {
        match Inline::new(b) {
            Some(inline) => PhpString(Repr::Inline(inline)),
            None => PhpString(Repr::Heap(Rc::new(b.to_vec()))),
        }
    }
}

impl PartialEq for PhpString {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        // Copies of one long string share a buffer, so equal pointers mean
        // equal bytes without a scan.
        if let (Repr::Heap(a), Repr::Heap(b)) = (&self.0, &other.0)
            && Rc::ptr_eq(a, b)
        {
            return true;
        }
        self.as_bytes() == other.as_bytes()
    }
}

impl Eq for PhpString {}

impl Ord for PhpString {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_bytes().cmp(other.as_bytes())
    }
}

impl PartialOrd for PhpString {
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
    #[inline(always)]
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
    fn short_strings_live_in_the_value() {
        for len in 0..=40usize {
            let bytes: Vec<u8> = (0..len as u8).map(|b| b'a' + b % 26).collect();
            let a = PhpString::from(bytes.as_slice());
            assert_eq!(a.as_bytes(), bytes.as_slice());
            assert_eq!(a.len(), len);
            assert_eq!(matches!(a.0, Repr::Inline(_)), len <= INLINE_CAPACITY);
            let b = PhpString::from(bytes.clone());
            assert!(a == b);
            let mut grown = PhpString::new();
            for &byte in &bytes {
                grown.push_bytes(&[byte]);
            }
            assert!(grown == a);
            assert_eq!(grown.into_bytes(), bytes);
        }
        let mut short = PhpString::from("abc");
        let copy = short.clone();
        short.make_mut_bytes()[0] = b'x';
        assert_eq!(&*short, b"xbc");
        assert_eq!(&*copy, b"abc");
        assert_eq!(copy.storage_owners(), 1);
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
