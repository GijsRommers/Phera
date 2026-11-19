//! PHP cryptography backed by RustCrypto.
//!
//! `openssl_digest('hello', 'sha256')` returns a lowercase hexadecimal digest.

use phera_extension_api::{Extension, Registry};

mod aead;
mod block;
mod cipher;
mod constants;
mod derive;

mod digest;
mod errors;
mod key;
mod key_details;
mod key_format;
mod key_functions;
mod key_input;

mod methods;
mod random;
mod rsa_encryption;

mod rsa_signatures;
mod signature;

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
            .chain(key_functions::FUNCTIONS)
            .chain(signature::FUNCTIONS)
            .chain(rsa_encryption::FUNCTIONS)
            .chain([&errors::ENTRY, &random::ENTRY, &derive::ENTRY])
        {
            registry.function(*entry);
        }
        constants::register(registry);
        key::register(registry);
    }
}
