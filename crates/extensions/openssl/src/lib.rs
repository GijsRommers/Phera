//! PHP cryptography backed by RustCrypto.
//!
//! `openssl_digest('hello', 'sha256')` returns a lowercase hexadecimal digest.

use phera_extension_api::{Extension, NativeFn, Registry};

mod aead;
mod block;
mod certificate;
mod cipher;
mod constants;
mod derive;
mod digest;
mod errors;
mod gcm;
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
        // How a fork of a snapshot gets this extension's request state.
        registry.fork_state::<crate::errors::ErrorQueue>(phera_extension_api::fresh_state);
        for entry in cipher::FUNCTIONS
            .iter()
            .chain(methods::FUNCTIONS)
            .chain(digest::FUNCTIONS)
            .chain(key_functions::FUNCTIONS)
            .chain(certificate::FUNCTIONS)
            .chain(signature::FUNCTIONS)
            .chain(rsa_encryption::FUNCTIONS)
            .chain([&errors::ENTRY, &random::ENTRY, &derive::ENTRY])
        {
            match entry.handler {
                NativeFn::Async(handler) if entry.params.iter().any(|param| param.sensitive) => {
                    registry.function_with_callback_frame(entry.name, entry.params, handler);
                }
                _ => registry.function(*entry),
            }
            registry.describe_function(entry.name, return_type(entry.name), None);
        }
        constants::register(registry);
        key::register(registry);
    }
}

fn return_type(name: &str) -> &'static str {
    match name {
        "openssl_cipher_iv_length" | "openssl_cipher_key_length" | "openssl_verify" => "int|false",
        "openssl_get_cipher_methods" | "openssl_get_md_methods" => "array",
        "openssl_random_pseudo_bytes" => "string",
        "openssl_pkey_get_details" => "array|false",
        "openssl_pkey_new"
        | "openssl_pkey_get_private"
        | "openssl_pkey_get_public"
        | "openssl_get_privatekey"
        | "openssl_get_publickey" => "OpenSSLAsymmetricKey|false",
        "openssl_x509_read" => "OpenSSLCertificate|false",
        "openssl_x509_verify" => "int",
        "openssl_x509_free" | "openssl_pkey_free" | "openssl_free_key" => "void",
        "openssl_sign"
        | "openssl_pkey_export"
        | "openssl_pkey_export_to_file"
        | "openssl_public_encrypt"
        | "openssl_private_encrypt"
        | "openssl_private_decrypt"
        | "openssl_x509_export"
        | "openssl_x509_check_private_key" => "bool",
        _ => "string|false",
    }
}
