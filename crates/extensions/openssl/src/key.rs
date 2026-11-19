//! `openssl_pkey_new(['private_key_type' => OPENSSL_KEYTYPE_ED25519])`
//! returns an opaque key object.

use std::rc::Rc;

use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, Context, MethodEntry, NativeClone, NativeError,
    NativeResult, Object, PhpArray, Registry, Value,
};
use rsa::rand_core::{TryCryptoRng, TryRng};

pub(crate) const CLASS: &str = "OpenSSLAsymmetricKey";

pub(crate) enum Private {
    Rsa(Box<rsa::PrivateKey>),
    P256(p256::SecretKey),
    P384(p384::SecretKey),
    P521(p521::SecretKey),
    Ed25519(ed25519_dalek::SigningKey),
    X25519(x25519_dalek::StaticSecret),
}

pub(crate) enum Public {
    Rsa(rsa::PublicKey),
    P256(p256::PublicKey),
    P384(p384::PublicKey),
    P521(p521::PublicKey),
    Ed25519(ed25519_dalek::VerifyingKey),
    X25519(x25519_dalek::PublicKey),
}

pub(crate) enum Key {
    Private(Private),
    Public(Public),
}

impl Private {
    pub(crate) fn public(&self) -> Public {
        match self {
            Self::Rsa(key) => Public::Rsa(key.to_public_key()),
            Self::P256(key) => Public::P256(key.public_key()),
            Self::P384(key) => Public::P384(key.public_key()),
            Self::P521(key) => Public::P521(key.public_key()),
            Self::Ed25519(key) => Public::Ed25519(key.verifying_key()),
            Self::X25519(key) => Public::X25519(key.into()),
        }
    }
}

pub(crate) struct OsRandom;

impl TryRng for OsRandom {
    type Error = getrandom::Error;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        getrandom::u32()
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        getrandom::u64()
    }

    fn try_fill_bytes(&mut self, bytes: &mut [u8]) -> Result<(), Self::Error> {
        getrandom::fill(bytes)
    }
}

impl TryCryptoRng for OsRandom {}

pub(crate) fn make(context: &mut dyn Context, key: Rc<Key>) -> NativeResult {
    let value = context.create_object(CLASS, PhpArray::new())?;
    if let Value::Object(object) = &value {
        *object.native.borrow_mut() = Some(Box::new(key));
    }
    Ok(value)
}

pub(crate) fn stored(object: &Object) -> Option<Rc<Key>> {
    object
        .native
        .borrow()
        .as_ref()?
        .downcast_ref::<Rc<Key>>()
        .cloned()
}

pub(crate) fn register(registry: &mut Registry) {
    const HOOKS: ClassHooks = ClassHooks {
        clone: Some(NativeClone::Forbidden),
        ..ClassHooks::EMPTY
    };

    for (class, constructor) in [
        (CLASS, CONSTRUCT_KEY),
        ("OpenSSLCertificate", CONSTRUCT_CERTIFICATE),
        ("OpenSSLCertificateSigningRequest", CONSTRUCT_CSR),
    ] {
        registry.class(ClassEntry {
            kind: ClassKind::FinalClass,
            methods: constructor,
            hooks: Some(&HOOKS),
            ..ClassEntry::class(class)
        });
    }
}

const CONSTRUCT_KEY: &[MethodEntry] = &[MethodEntry::public("__construct", &[], construct_key)];
const CONSTRUCT_CERTIFICATE: &[MethodEntry] = &[MethodEntry::public(
    "__construct",
    &[],
    construct_certificate,
)];
const CONSTRUCT_CSR: &[MethodEntry] = &[MethodEntry::public("__construct", &[], construct_csr)];

fn construct_key(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    denied_constructor(CLASS, "openssl_pkey_new")
}

fn construct_certificate(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    denied_constructor("OpenSSLCertificate", "openssl_x509_read")
}

fn construct_csr(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    denied_constructor("OpenSSLCertificateSigningRequest", "openssl_csr_new")
}

fn denied_constructor(class: &str, alternative: &str) -> NativeResult {
    Err(NativeError::error(format!(
        "Cannot directly construct {class}, use {alternative}() instead"
    )))
}
