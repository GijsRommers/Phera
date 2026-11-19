//! `serialize("a\0b")` writes the NUL byte verbatim and counts it in the length.

#[derive(Default)]
pub struct Writer(Vec<u8>);

impl Writer {
    pub fn finish(self) -> Vec<u8> {
        self.0
    }

    pub fn null(&mut self) {
        self.0.extend_from_slice(b"N;");
    }

    pub fn boolean(&mut self, value: bool) {
        self.0
            .extend_from_slice(if value { b"b:1;" } else { b"b:0;" });
    }

    pub fn integer(&mut self, value: i64) {
        self.0.extend_from_slice(b"i:");
        self.number(value);
        self.0.push(b';');
    }

    pub fn float(&mut self, decimal: &str) {
        self.0.extend_from_slice(b"d:");
        self.0.extend_from_slice(decimal.as_bytes());
        self.0.push(b';');
    }

    /// The exact decimal for a small dyadic fraction needs at most four places.
    /// Below 2^38 the rounding interval is smaller than the gap to any
    /// shorter decimal for these fractions.
    pub fn short_float(&mut self, value: f64) -> bool {
        let scaled = value.abs() * 16.0;
        let units = scaled as u64;
        if units >= 1 << 42 || units as f64 != scaled {
            return false;
        }
        self.0.extend_from_slice(b"d:");
        if value.is_sign_negative() {
            self.0.push(b'-');
        }
        self.number(units / 16);
        let fraction = units % 16;
        if fraction != 0 {
            let mut digits = [b'0'; 4];
            let mut decimal = fraction * 625;
            for digit in digits.iter_mut().rev() {
                *digit += (decimal % 10) as u8;
                decimal /= 10;
            }
            let mut length = digits.len();
            while length > 0 && digits[length - 1] == b'0' {
                length -= 1;
            }
            self.0.push(b'.');
            self.0.extend_from_slice(&digits[..length]);
        }
        self.0.push(b';');
        true
    }

    pub fn string(&mut self, value: &[u8]) {
        self.quoted(b's', value);
        self.0.push(b';');
    }

    pub fn array(&mut self, count: usize) {
        self.0.extend_from_slice(b"a:");
        self.open(count);
    }

    pub fn object(&mut self, class: &[u8], count: usize) {
        self.quoted(b'O', class);
        self.0.push(b':');
        self.open(count);
    }

    /// Writes a declared property's pre-encoded key.
    pub fn property(&mut self, encoded_key: &[u8]) {
        self.0.extend_from_slice(encoded_key);
    }

    pub fn custom_object(&mut self, class: &[u8], payload: &[u8]) {
        self.quoted(b'C', class);
        self.0.push(b':');
        self.open(payload.len());
        self.0.extend_from_slice(payload);
        self.close();
    }

    pub fn enum_case(&mut self, class: &[u8], case: &[u8]) {
        self.0.extend_from_slice(b"E:");
        self.number(class.len() + 1 + case.len());
        self.0.extend_from_slice(b":\"");
        self.0.extend_from_slice(class);
        self.0.push(b':');
        self.0.extend_from_slice(case);
        self.0.extend_from_slice(b"\";");
    }

    pub fn reference(&mut self, slot: usize) {
        self.0.extend_from_slice(b"R:");
        self.number(slot);
        self.0.push(b';');
    }

    pub fn object_reference(&mut self, slot: usize) {
        self.0.extend_from_slice(b"r:");
        self.number(slot);
        self.0.push(b';');
    }

    pub fn close(&mut self) {
        self.0.push(b'}');
    }

    fn open(&mut self, count: usize) {
        self.number(count);
        self.0.extend_from_slice(b":{");
    }

    fn number(&mut self, value: impl itoa::Integer) {
        self.0
            .extend_from_slice(itoa::Buffer::new().format(value).as_bytes());
    }

    fn quoted(&mut self, kind: u8, bytes: &[u8]) {
        self.0.reserve(bytes.len() + 24);
        self.0.extend_from_slice(&[kind, b':']);
        self.number(bytes.len());
        self.0.extend_from_slice(b":\"");
        self.0.extend_from_slice(bytes);
        self.0.push(b'"');
    }
}
