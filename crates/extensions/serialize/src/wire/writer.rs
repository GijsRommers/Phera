//! `serialize("a\0b")` writes the NUL byte verbatim and counts it in the length.

use std::io::Write;

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
        let _ = write!(&mut self.0, "i:{value};");
    }

    pub fn float(&mut self, decimal: &str) {
        self.0.extend_from_slice(b"d:");
        self.0.extend_from_slice(decimal.as_bytes());
        self.0.push(b';');
    }

    pub fn string(&mut self, value: &[u8]) {
        self.quoted('s', value);
        self.0.push(b';');
    }

    pub fn array(&mut self, count: usize) {
        let _ = write!(&mut self.0, "a:{count}:{{");
    }

    pub fn object(&mut self, class: &[u8], count: usize) {
        self.quoted('O', class);
        let _ = write!(&mut self.0, ":{count}:{{");
    }

    pub fn custom_object(&mut self, class: &[u8], payload: &[u8]) {
        self.quoted('C', class);
        let _ = write!(&mut self.0, ":{}:{{", payload.len());
        self.0.extend_from_slice(payload);
        self.close();
    }

    pub fn enum_case(&mut self, class: &[u8], case: &[u8]) {
        let _ = write!(&mut self.0, "E:{}:\"", class.len() + 1 + case.len());
        self.0.extend_from_slice(class);
        self.0.push(b':');
        self.0.extend_from_slice(case);
        self.0.extend_from_slice(b"\";");
    }

    pub fn reference(&mut self, slot: usize) {
        let _ = write!(&mut self.0, "R:{slot};");
    }

    pub fn object_reference(&mut self, slot: usize) {
        let _ = write!(&mut self.0, "r:{slot};");
    }

    pub fn close(&mut self) {
        self.0.push(b'}');
    }

    fn quoted(&mut self, kind: char, bytes: &[u8]) {
        let _ = write!(&mut self.0, "{kind}:{}:\"", bytes.len());
        self.0.extend_from_slice(bytes);
        self.0.push(b'"');
    }
}
