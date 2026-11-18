//! HMAC (RFC 2104) over any cryptographic algorithm, for `hash_hmac()`,
//! `hash_init()` with `HASH_HMAC`, and the key derivations:
//!
//! ```php
//! hash_hmac('sha256', 'message', 'key');
//! // "6e9ef29b75fffc5b7abae527d58fdadb2fe42e7219011976917343065f58ed4a"
//! ```

use crate::algorithms::Algorithm;

/// What RFC 2104 XORs the key with, for the inner and the outer digest.
const INNER: u8 = 0x36;
const OUTER: u8 = 0x5c;

/// A key prepared once, for any number of messages.
#[derive(Clone)]
pub struct Hmac {
    algorithm: Algorithm,
    /// The key padded to a block and XORed with [`INNER`]; the inner digest
    /// starts with it.
    pub inner_pad: Vec<u8>,
    outer_pad: Vec<u8>,
}

impl Hmac {
    /// `algorithm` must be cryptographic, which the functions that take
    /// one check first: only those have a block size.
    pub fn new(algorithm: Algorithm, key: &[u8]) -> Hmac {
        let block = algorithm
            .block_size()
            .expect("HMAC is only offered for cryptographic algorithms");
        let hashed;
        let key = if key.len() > block {
            // A key longer than a block is replaced by its digest.
            hashed = algorithm.digest(key);
            &hashed
        } else {
            key
        };
        let mut inner_pad = vec![INNER; block];
        let mut outer_pad = vec![OUTER; block];
        for ((inner, outer), byte) in inner_pad.iter_mut().zip(&mut outer_pad).zip(key) {
            *inner ^= byte;
            *outer ^= byte;
        }
        Hmac {
            algorithm,
            inner_pad,
            outer_pad,
        }
    }

    /// The HMAC of `message`.
    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        let mut inner = self.inner_pad.clone();
        inner.extend_from_slice(message);
        self.finish(&self.algorithm.digest(&inner))
    }

    /// The HMAC from the digest of the inner pad followed by the message,
    /// for a caller that computed that digest piece by piece.
    pub fn finish(&self, inner_digest: &[u8]) -> Vec<u8> {
        let mut outer = self.outer_pad.clone();
        outer.extend_from_slice(inner_digest);
        self.algorithm.digest(&outer)
    }
}
