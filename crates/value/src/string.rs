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

    /// Borrows mutable bytes only when this PHP string owns its buffer.
    /// A secure erase can preserve the bytes visible to other PHP values.
    pub fn as_unique_bytes_mut(&mut self) -> Option<&mut [u8]> {
        Rc::get_mut(&mut self.0).map(Vec::as_mut_slice)
    }

    /// Mutate the bytes, preserving other PHP values that share this buffer.
    pub fn make_mut_bytes(&mut self) -> &mut [u8] {
        Rc::make_mut(&mut self.0).as_mut_slice()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Number of values sharing the backing storage, for `debug_zval_dump()`.
    pub fn storage_owners(&self) -> usize {
        Rc::strong_count(&self.0)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(&self.0).into_owned()
    }
}

/// Every number from 00 to 99 as two digits, for writing an integer two at a time.
const PAIRS: &[u8; 200] = b"0001020304050607080910111213141516171819\
2021222324252627282930313233343536373839\
4041424344454647484950515253545556575859\
6061626364656667686970717273747576777879\
8081828384858687888990919293949596979899";

/// The decimal digits of `number` at the end of the returned array, and
/// where they start.
#[inline]
fn integer_digits(number: i64) -> ([u8; 20], usize) {
    let mut digits = [0u8; 20];
    let mut at = digits.len();
    let mut rest = number.unsigned_abs();
    while rest >= 100 {
        let pair = (rest % 100) as usize * 2;
        rest /= 100;
        at -= 2;
        digits[at] = PAIRS[pair];
        digits[at + 1] = PAIRS[pair + 1];
    }
    if rest >= 10 {
        let pair = rest as usize * 2;
        at -= 2;
        digits[at] = PAIRS[pair];
        digits[at + 1] = PAIRS[pair + 1];
    } else {
        at -= 1;
        digits[at] = b'0' + rest as u8;
    }
    if number < 0 {
        at -= 1;
        digits[at] = b'-';
    }
    (digits, at)
}

/// Appends `number` in decimal to `out`, as `(string) $number` writes it,
/// without going through a formatter.
pub fn push_integer(out: &mut Vec<u8>, number: i64) {
    let (digits, at) = integer_digits(number);
    out.extend_from_slice(&digits[at..]);
}

impl PhpString {
    /// `(string) $number` for an integer, in one buffer of its length.
    pub fn from_integer(number: i64) -> Self {
        let (digits, at) = integer_digits(number);
        PhpString(Rc::new(digits[at..].to_vec()))
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

#[cfg(test)]
mod integer_tests {
    use super::*;

    /// Every integer reads as `i64::to_string` writes it, at the edges too.
    #[test]
    fn integers_are_written_as_the_formatter_writes_them() {
        for number in [
            0,
            1,
            9,
            10,
            99,
            100,
            12_345,
            -1,
            -10,
            -987_654_321,
            i64::MAX,
            i64::MIN,
            i64::MIN + 1,
            1_000_000_000_000,
        ]
        .into_iter()
        .chain(-2_500..2_500)
        .chain((0..19).flat_map(|power| {
            let ten = 10_i64.pow(power);
            [ten - 1, ten, ten + 1, -ten]
        })) {
            assert_eq!(
                PhpString::from_integer(number).as_bytes(),
                number.to_string().as_bytes()
            );
            let mut out = b"x".to_vec();
            push_integer(&mut out, number);
            assert_eq!(out, format!("x{number}").into_bytes());
        }
    }
}
