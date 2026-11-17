//! Ordered byte strings shared by native startup and PHP's `getenv()`.

use indexmap::IndexMap;

/// An environment in enumeration order. Deliberately has no Debug implementation.
#[derive(Clone, Default)]
pub struct Environment(IndexMap<Vec<u8>, Vec<u8>>);

impl Environment {
    pub fn process() -> Self {
        let mut env = Self::default();
        for (name, value) in std::env::vars_os() {
            env.insert(name.into_encoded_bytes(), value.into_encoded_bytes());
        }
        env
    }

    pub fn insert(&mut self, name: Vec<u8>, value: Vec<u8>) {
        self.0.insert(name, value);
    }

    pub fn get(&self, name: &[u8]) -> Option<&[u8]> {
        self.0.get(name).map(Vec::as_slice)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&[u8], &[u8])> {
        self.0.iter().map(|(k, v)| (k.as_slice(), v.as_slice()))
    }
}
