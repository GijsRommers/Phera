//! PHP cryptography backed by RustCrypto.
//!
//! `openssl_digest('hello', 'sha256')` returns a lowercase hexadecimal digest.

use phera_extension_api::{Extension, Registry};

mod aead;
mod block;
mod cipher;
mod constants;
mod digest;
mod errors;
mod methods;
mod random;

pub struct OpenSsl;

impl Extension for OpenSsl {
    fn name(&self) -> &'static str {
        "openssl"
    }

    fn register(&self, registry: &mut Registry) {
        for entry in cipher::FUNCTIONS
            .iter()
            .chain(methods::FUNCTIONS)
            .chain(digest::FUNCTIONS)
            .chain([&errors::ENTRY, &random::ENTRY])
        {
            registry.function(*entry);
        }
        constants::register(registry);
    }
}
