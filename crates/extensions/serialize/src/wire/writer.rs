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
