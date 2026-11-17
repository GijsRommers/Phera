//! PHP's hash extension: digests, HMAC, key derivation, incremental
//! hashing, and the password functions:
//!
//! ```php
//! hash('sha256', 'hello');            // "2cf24dba5fb0a30e26e83b2ac5b9e29e..."
//! md5('hello', true);                 // 16 raw bytes
//! $h = hash_init('sha256');
//! hash_update($h, 'hel');
//! hash_update($h, 'lo');
//! hash_final($h);                     // the same digest as hash('sha256', 'hello')
//! password_verify('secret', password_hash('secret', PASSWORD_DEFAULT));  // true
//! ```
//!
//! `crc32()` itself belongs to the standard extension.

mod algorithms;
mod digests;
mod files;
mod hmac;
mod incremental;
mod kdf;
mod options;
mod password_info;
mod password_options;
mod passwords;

use phera_extension_api::{Extension, Registry, Value};

pub struct Hash;

impl Extension for Hash {
    fn name(&self) -> &'static str {
        "hash"
    }

    fn register(&self, registry: &mut Registry) {
        registry.constant("HASH_HMAC", incremental::HASH_HMAC);
        for &(name, value) in passwords::STRING_CONSTANTS {
            registry.constant(name, value);
        }
        for &(name, value) in passwords::INT_CONSTANTS {
            registry.constant(name, value);
        }
        registry.class(incremental::HASH_CONTEXT);
        for functions in [
            digests::FUNCTIONS,
            kdf::FUNCTIONS,
            incremental::FUNCTIONS,
            files::FUNCTIONS,
            passwords::FUNCTIONS,
            password_info::FUNCTIONS,
        ] {
            for &entry in functions {
                registry.function(entry);
            }
        }
    }
}

/// How PHP names a wrong argument in a `TypeError`: `true` and `false`
/// rather than `bool`, otherwise the type or the class.
fn given(value: &Value) -> String {
    match value {
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        other => other.type_name(),
    }
}
