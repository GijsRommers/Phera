//! `openssl_pkey_export($key, $pem, 'passphrase')` writes encrypted PKCS#8 PEM.

use pkcs8::der::{
    Decode, Document, Encode, SecretDocument,
    asn1::{BitStringRef, OctetStringRef},
};
use pkcs8::spki::{AlgorithmIdentifierRef, SubjectPublicKeyInfoRef};
use pkcs8::{
    DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey,
    EncryptedPrivateKeyInfoRef, LineEnding, ObjectIdentifier, PrivateKeyInfoRef,
};
use rsa::pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey};
use rsa::rand_core::TryRng;

use crate::key::{OsRandom, Private, Public};

const X25519: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.101.110");
const X25519_ALGORITHM: AlgorithmIdentifierRef<'static> = AlgorithmIdentifierRef {
    oid: X25519,
    parameters: None,
};

pub(crate) fn private(bytes: &[u8], password: &[u8]) -> Option<Private> {
    let bytes = bytes.trim_ascii_start();
    if bytes.starts_with(b"-----") {
        let (label, document) = SecretDocument::from_pem(std::str::from_utf8(bytes).ok()?).ok()?;
        return match label {
            "PRIVATE KEY" => private_pkcs8(document.as_bytes()),
            "ENCRYPTED PRIVATE KEY" => private_encrypted(document.as_bytes(), password),
            "RSA PRIVATE KEY" => rsa::PrivateKey::from_pkcs1_der(document.as_bytes())
                .ok()
                .map(Box::new)
                .map(Private::Rsa),
            "EC PRIVATE KEY" => private_sec1(document.as_bytes()),
            _ => None,
        };
    }
    private_pkcs8(bytes)
        .or_else(|| private_encrypted(bytes, password))
        .or_else(|| {
            rsa::PrivateKey::from_pkcs1_der(bytes)
                .ok()
                .map(Box::new)
                .map(Private::Rsa)
        })
        .or_else(|| private_sec1(bytes))
}

fn private_pkcs8(bytes: &[u8]) -> Option<Private> {
    rsa::PrivateKey::from_pkcs8_der(bytes)
        .ok()
        .map(Box::new)
        .map(Private::Rsa)
        .or_else(|| {
            p256::SecretKey::from_pkcs8_der(bytes)
                .ok()
                .map(Private::P256)
        })
        .or_else(|| {
            p384::SecretKey::from_pkcs8_der(bytes)
                .ok()
                .map(Private::P384)
        })
        .or_else(|| {
            p521::SecretKey::from_pkcs8_der(bytes)
                .ok()
                .map(Private::P521)
        })
        .or_else(|| {
            ed25519_dalek::SigningKey::from_pkcs8_der(bytes)
                .ok()
                .map(Private::Ed25519)
        })
        .or_else(|| x25519_private(bytes).map(Private::X25519))
}

fn private_sec1(bytes: &[u8]) -> Option<Private> {
    use sec1::EcPrivateKey;
    let structure = EcPrivateKey::from_der(bytes).ok()?;
    let curve = structure.parameters?.named_curve()?;
    match curve.to_string().as_str() {
        "1.2.840.10045.3.1.7" => p256::SecretKey::from_sec1_der(bytes)
            .ok()
            .map(Private::P256),
        "1.3.132.0.34" => p384::SecretKey::from_sec1_der(bytes)
            .ok()
            .map(Private::P384),
        "1.3.132.0.35" => p521::SecretKey::from_sec1_der(bytes)
            .ok()
            .map(Private::P521),
        _ => None,
    }
}

fn private_encrypted(bytes: &[u8], password: &[u8]) -> Option<Private> {
    let info = EncryptedPrivateKeyInfoRef::from_der(bytes).ok()?;
    let document = info.decrypt(password).ok()?;
    private_pkcs8(document.as_bytes())
}

fn x25519_private(bytes: &[u8]) -> Option<x25519_dalek::StaticSecret> {
    let info = PrivateKeyInfoRef::from_der(bytes).ok()?;
    if info.algorithm != X25519_ALGORITHM {
        return None;
    }
    let nested = <&OctetStringRef>::from_der(info.private_key.as_bytes()).ok()?;
    Some(<[u8; 32]>::try_from(nested.as_bytes()).ok()?.into())
}

pub(crate) fn public(bytes: &[u8]) -> Option<Public> {
    let bytes = bytes.trim_ascii_start();
    if bytes.starts_with(b"-----") {
        let (label, document) = Document::from_pem(std::str::from_utf8(bytes).ok()?).ok()?;
        return match label {
            "PUBLIC KEY" => public_spki(document.as_bytes()),
            "RSA PUBLIC KEY" => rsa::PublicKey::from_pkcs1_der(document.as_bytes())
                .ok()
                .map(Public::Rsa),
            _ => None,
        };
    }
    public_spki(bytes).or_else(|| rsa::PublicKey::from_pkcs1_der(bytes).ok().map(Public::Rsa))
}

pub(crate) fn public_spki(bytes: &[u8]) -> Option<Public> {
    rsa::PublicKey::from_public_key_der(bytes)
        .ok()
        .map(Public::Rsa)
        .or_else(|| {
            p256::PublicKey::from_public_key_der(bytes)
                .ok()
                .map(Public::P256)
        })
        .or_else(|| {
            p384::PublicKey::from_public_key_der(bytes)
                .ok()
                .map(Public::P384)
        })
        .or_else(|| {
            p521::PublicKey::from_public_key_der(bytes)
                .ok()
                .map(Public::P521)
        })
        .or_else(|| {
            ed25519_dalek::VerifyingKey::from_public_key_der(bytes)
                .ok()
                .map(Public::Ed25519)
        })
        .or_else(|| x25519_public(bytes).map(Public::X25519))
}

fn x25519_public(bytes: &[u8]) -> Option<x25519_dalek::PublicKey> {
    let info = SubjectPublicKeyInfoRef::from_der(bytes).ok()?;
    if info.algorithm != X25519_ALGORITHM {
        return None;
    }
    Some(
        <[u8; 32]>::try_from(info.subject_public_key.as_bytes()?)
            .ok()?
            .into(),
    )
}

pub(crate) fn public_pem(key: &Public) -> Option<String> {
    let pem = match key {
        Public::Rsa(key) => key.to_public_key_pem(LineEnding::LF),
        Public::P256(key) => key.to_public_key_pem(LineEnding::LF),
        Public::P384(key) => key.to_public_key_pem(LineEnding::LF),
        Public::P521(key) => key.to_public_key_pem(LineEnding::LF),
        Public::Ed25519(key) => key.to_public_key_pem(LineEnding::LF),
        Public::X25519(key) => {
            let info = SubjectPublicKeyInfoRef {
                algorithm: X25519_ALGORITHM,
                subject_public_key: BitStringRef::from_bytes(key.as_bytes()).ok()?,
            };
            return Document::encode_msg(&info)
                .ok()?
                .to_pem("PUBLIC KEY", LineEnding::LF)
                .ok();
        }
    };
    pem.ok()
}

fn private_der(key: &Private) -> Option<SecretDocument> {
    match key {
        Private::Rsa(key) => key.to_pkcs8_der().ok(),
        Private::P256(key) => key.to_pkcs8_der().ok(),
        Private::P384(key) => key.to_pkcs8_der().ok(),
        Private::P521(key) => key.to_pkcs8_der().ok(),
        Private::Ed25519(key) => key.to_pkcs8_der().ok(),
        Private::X25519(key) => {
            let nested = OctetStringRef::new(key.as_bytes()).ok()?.to_der().ok()?;
            let info = PrivateKeyInfoRef::new(X25519_ALGORITHM, OctetStringRef::new(&nested).ok()?);
            SecretDocument::encode_msg(&info).ok()
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Encryption {
    TripleDes,
    Aes128,
    Aes192,
    Aes256,
}

impl Encryption {
    fn parameters(self) -> Option<pkcs8::pkcs5::pbes2::Parameters> {
        use pkcs8::pkcs5::pbes2::{EncryptionScheme, Parameters, Pbkdf2Params};

        let mut salt = [0; 16];
        let mut iv = [0; 16];
        OsRandom.try_fill_bytes(&mut salt).ok()?;
        OsRandom.try_fill_bytes(&mut iv).ok()?;
        let kdf = Pbkdf2Params::hmac_sha256(2048, &salt).ok()?.into();
        let encryption = match self {
            Self::TripleDes => EncryptionScheme::DesEde3Cbc {
                iv: iv[..8].try_into().ok()?,
            },
            Self::Aes128 => EncryptionScheme::Aes128Cbc { iv },
            Self::Aes192 => EncryptionScheme::Aes192Cbc { iv },
            Self::Aes256 => EncryptionScheme::Aes256Cbc { iv },
        };
        Some(Parameters { kdf, encryption })
    }
}

pub(crate) fn export(key: &Private, encryption: Option<(&[u8], Encryption)>) -> Option<Vec<u8>> {
    let document = private_der(key)?;
    let (label, document) = match encryption {
        None => ("PRIVATE KEY", document),
        Some((password, cipher)) => {
            let info = PrivateKeyInfoRef::from_der(document.as_bytes()).ok()?;
            let encrypted = info
                .encrypt_with_params(cipher.parameters()?, password)
                .ok()?;
            ("ENCRYPTED PRIVATE KEY", encrypted)
        }
    };
    Some(
        document
            .to_pem(label, LineEnding::LF)
            .ok()?
            .as_bytes()
            .to_vec(),
    )
}
