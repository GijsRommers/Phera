use std::fmt;
use std::rc::Rc;

/// A PHP string: an arbitrary byte string (need not be UTF-8), cheap to clone
/// and copy-on-write, with amortised O(1) append for `.=`.
#[derive(Default, Clone)]
pub struct PhpString(Rc<Vec<u8>>);

impl PhpString {
    pub fn new() -> Self {
        PhpString(Rc::new(Vec::new()))
    }

    /// Appends bytes, cloning the backing buffer only when shared.
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        Rc::make_mut(&mut self.0).extend_from_slice(bytes);
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(&self.0).into_owned()
    }
}

impl From<&str> for PhpString {
    fn from(s: &str) -> Self {
        PhpString(Rc::new(s.as_bytes().to_vec()))
    }
}

impl From<String> for PhpString {
    fn from(s: String) -> Self {
        PhpString(Rc::new(s.into_bytes()))
    }
}

impl From<Vec<u8>> for PhpString {
    fn from(b: Vec<u8>) -> Self {
        PhpString(Rc::new(b))
    }
}

impl From<&[u8]> for PhpString {
    fn from(b: &[u8]) -> Self {
        PhpString(Rc::new(b.to_vec()))
    }
}

impl PartialEq for PhpString {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for PhpString {}

impl Ord for PhpString {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialOrd for PhpString {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::hash::Hash for PhpString {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl std::ops::Deref for PhpString {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
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
}
