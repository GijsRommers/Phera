//! `openssl_pkey_get_details($key)['key']` gives the matching public PEM.

use p256::elliptic_curve::sec1::ToSec1Point;
use phera_extension_api::{ArrayKey, Context, NativeResult, PhpArray, Value, args};
use rsa::traits::{PrivateKeyParts, PublicKeyParts};

use crate::key::{self, Key, Private, Public};
use crate::key_format;

pub(crate) fn details(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let Some(key) = args::object(arguments, 0).and_then(|object| key::stored(object)) else {
        return Ok(Value::Bool(false));
    };
    let private;
    let public = match &*key {
        Key::Private(key) => {
            private = key.public();
            &private
        }
        Key::Public(key) => key,
    };
    let Some(pem) = key_format::public_pem(public) else {
        return Ok(Value::Bool(false));
    };
    let (bits, kind, label, details) = describe(public, &key);
    let mut output = PhpArray::new();
    entry(&mut output, "bits", bits);
    entry(&mut output, "key", pem);
    entry(&mut output, label, details);
    entry(&mut output, "type", kind);
    Ok(Value::from(output))
}

fn describe(public: &Public, key: &Key) -> (i64, i64, &'static str, PhpArray) {
    match public {
        Public::Rsa(public) => {
            let private = match key {
                Key::Private(Private::Rsa(private)) => Some(private.as_ref()),
                _ => None,
            };
            (
                i64::from(public.n().bit_width_vartime()),
                0,
                "rsa",
                rsa(public, private),
            )
        }
        Public::P256(public) => {
            let point = public.to_sec1_point(false);
            let secret = match key {
                Key::Private(Private::P256(key)) => Some(key.to_bytes()),
                _ => None,
            };
            let details = ec(
                "prime256v1",
                "1.2.840.10045.3.1.7",
                point.x().map(|bytes| &bytes[..]),
                point.y().map(|bytes| &bytes[..]),
                secret.as_ref().map(|bytes| &bytes[..]),
            );
            (256, 3, "ec", details)
        }
        Public::P384(public) => {
            let point = public.to_sec1_point(false);
            let secret = match key {
                Key::Private(Private::P384(key)) => Some(key.to_bytes()),
                _ => None,
            };
            let details = ec(
                "secp384r1",
                "1.3.132.0.34",
                point.x().map(|bytes| &bytes[..]),
                point.y().map(|bytes| &bytes[..]),
                secret.as_ref().map(|bytes| &bytes[..]),
            );
            (384, 3, "ec", details)
        }
        Public::P521(public) => {
            let point = public.to_sec1_point(false);
            let secret = match key {
                Key::Private(Private::P521(key)) => Some(key.to_bytes()),
                _ => None,
            };
            let details = ec(
                "secp521r1",
                "1.3.132.0.35",
                point.x().map(|bytes| &bytes[..]),
                point.y().map(|bytes| &bytes[..]),
                secret.as_ref().map(|bytes| &bytes[..]),
            );
            (521, 3, "ec", details)
        }
        Public::X25519(public) => {
            let private = match key {
                Key::Private(Private::X25519(key)) => Some(key.as_bytes().as_slice()),
                _ => None,
            };
            (253, 4, "x25519", raw(public.as_bytes(), private))
        }
        Public::Ed25519(public) => {
            let private = match key {
                Key::Private(Private::Ed25519(key)) => Some(key.as_bytes().as_slice()),
                _ => None,
            };
            (256, 5, "ed25519", raw(public.as_bytes(), private))
        }
    }
}

fn entry(array: &mut PhpArray, name: &str, value: impl Into<Value>) {
    array.insert(ArrayKey::str(name), value.into());
}

fn rsa(public: &rsa::PublicKey, private: Option<&rsa::PrivateKey>) -> PhpArray {
    let mut output = PhpArray::new();
    integer(&mut output, "n", public.n());
    integer(&mut output, "e", public.e());
    if let Some(private) = private {
        integer(&mut output, "d", private.d());
        let primes = private.primes();
        for (name, value) in [
            ("p", primes.first().map(|number| number.as_ref())),
            ("q", primes.get(1).map(|number| number.as_ref())),
            ("dmp1", private.dp()),
            ("dmq1", private.dq()),
        ] {
            if let Some(value) = value {
                integer(&mut output, name, value);
            }
        }
        if let Some(coefficient) = private.crt_coefficient() {
            integer(&mut output, "iqmp", &coefficient);
        }
    }
    output
}

fn integer(output: &mut PhpArray, name: &str, number: &rsa::BoxedUint) {
    entry(
        output,
        name,
        number.to_be_bytes_trimmed_vartime().into_vec(),
    );
}

fn ec(
    name: &'static str,
    oid: &'static str,
    x: Option<&[u8]>,
    y: Option<&[u8]>,
    private: Option<&[u8]>,
) -> PhpArray {
    let mut output = PhpArray::new();
    entry(&mut output, "curve_name", name);
    entry(&mut output, "curve_oid", oid);
    for (name, bytes) in [("x", x), ("y", y), ("d", private)] {
        if let Some(bytes) = bytes {
            let first = bytes
                .iter()
                .position(|byte| *byte != 0)
                .unwrap_or(bytes.len());
            entry(&mut output, name, bytes[first..].to_vec());
        }
    }
    output
}

fn raw(public: &[u8], private: Option<&[u8]>) -> PhpArray {
    let mut output = PhpArray::new();
    if let Some(private) = private {
        entry(&mut output, "priv_key", private.to_vec());
    }
    entry(&mut output, "pub_key", public.to_vec());
    output
}
