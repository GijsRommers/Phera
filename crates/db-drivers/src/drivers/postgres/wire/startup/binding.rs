//! RFC 5929 hashes the exact DER certificate from the TLS handshake.

use pkcs1::RsaPssParams;
use sha2::{Digest, Sha224, Sha256, Sha384, Sha512, Sha512_224, Sha512_256};
use sha3::{Sha3_224, Sha3_256, Sha3_384, Sha3_512};
use x509_cert::{
    Certificate,
    der::{Decode, asn1::Any},
};

pub(super) fn certificate_hash(der: &[u8]) -> Option<Vec<u8>> {
    let certificate = Certificate::from_der(der).ok()?;
    let signature = certificate.signature_algorithm();
    let hash_oid = match signature.oid.to_string().as_str() {
        // Legacy MD5 and SHA-1 signatures use SHA-256 for the binding.
        "1.2.840.113549.1.1.4"
        | "1.2.840.113549.1.1.5"
        | "1.2.840.10040.4.3"
        | "1.2.840.10045.4.1"
        | "1.3.14.3.2.29"
        | "1.2.840.113549.1.1.11"
        | "1.2.840.10045.4.3.2"
        | "2.16.840.1.101.3.4.3.2" => {
            return Some(Sha256::digest(der).to_vec());
        }
        "1.2.840.113549.1.1.14" | "1.2.840.10045.4.3.1" | "2.16.840.1.101.3.4.3.1" => {
            return Some(Sha224::digest(der).to_vec());
        }
        "1.2.840.113549.1.1.12" | "1.2.840.10045.4.3.3" | "2.16.840.1.101.3.4.3.3" => {
            return Some(Sha384::digest(der).to_vec());
        }
        "1.2.840.113549.1.1.13" | "1.2.840.10045.4.3.4" | "2.16.840.1.101.3.4.3.4" => {
            return Some(Sha512::digest(der).to_vec());
        }
        "1.2.840.113549.1.1.15" => return Some(Sha512_224::digest(der).to_vec()),
        "1.2.840.113549.1.1.16" => return Some(Sha512_256::digest(der).to_vec()),
        "2.16.840.1.101.3.4.3.5" | "2.16.840.1.101.3.4.3.9" | "2.16.840.1.101.3.4.3.13" => {
            return Some(Sha3_224::digest(der).to_vec());
        }
        "2.16.840.1.101.3.4.3.6" | "2.16.840.1.101.3.4.3.10" | "2.16.840.1.101.3.4.3.14" => {
            return Some(Sha3_256::digest(der).to_vec());
        }
        "2.16.840.1.101.3.4.3.7" | "2.16.840.1.101.3.4.3.11" | "2.16.840.1.101.3.4.3.15" => {
            return Some(Sha3_384::digest(der).to_vec());
        }
        "2.16.840.1.101.3.4.3.8" | "2.16.840.1.101.3.4.3.12" | "2.16.840.1.101.3.4.3.16" => {
            return Some(Sha3_512::digest(der).to_vec());
        }
        "1.2.840.113549.1.1.10" => {
            let params = signature
                .parameters
                .as_ref()?
                .decode_as::<RsaPssParams<Any>>()
                .ok()?;
            params.hash.oid
        }
        _ => return None,
    };
    match hash_oid.to_string().as_str() {
        "1.3.14.3.2.26" | "1.2.840.113549.2.5" | "2.16.840.1.101.3.4.2.1" => {
            Some(Sha256::digest(der).to_vec())
        }
        "2.16.840.1.101.3.4.2.2" => Some(Sha384::digest(der).to_vec()),
        "2.16.840.1.101.3.4.2.3" => Some(Sha512::digest(der).to_vec()),
        "2.16.840.1.101.3.4.2.4" => Some(Sha224::digest(der).to_vec()),
        "2.16.840.1.101.3.4.2.5" => Some(Sha512_224::digest(der).to_vec()),
        "2.16.840.1.101.3.4.2.6" => Some(Sha512_256::digest(der).to_vec()),
        "2.16.840.1.101.3.4.2.7" => Some(Sha3_224::digest(der).to_vec()),
        "2.16.840.1.101.3.4.2.8" => Some(Sha3_256::digest(der).to_vec()),
        "2.16.840.1.101.3.4.2.9" => Some(Sha3_384::digest(der).to_vec()),
        "2.16.840.1.101.3.4.2.10" => Some(Sha3_512::digest(der).to_vec()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_uses_the_certificate_signature_hash() {
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P384_SHA384).unwrap();
        let certificate = rcgen::CertificateParams::new(vec!["localhost".into()])
            .unwrap()
            .self_signed(&key)
            .unwrap();
        let der = certificate.der();
        let expected = ring::digest::digest(&ring::digest::SHA384, der);
        assert_eq!(certificate_hash(der).as_deref(), Some(expected.as_ref()));
    }
}
