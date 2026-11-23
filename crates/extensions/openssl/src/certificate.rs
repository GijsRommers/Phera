//! `openssl_x509_fingerprint($pem, 'sha256')` fingerprints a certificate's DER.

use std::rc::Rc;

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeResult, Object, Param, ParamType,
    PhpArray, Value, args,
};
use pkcs8::der::{Decode, DecodePem, Encode, EncodePem};
use pkcs8::{LineEnding, ObjectIdentifier};
use subtle::ConstantTimeEq;
use x509_cert::Certificate;

use crate::digest::Algorithm;
use crate::errors;
use crate::key::{Key, Public};
use crate::key_format;
use crate::key_input::{self, Role};
use crate::signature::verify_certificate_bytes;

const CLASS: &str = "OpenSSLCertificate";
const CERTIFICATE: Param = Param::new("certificate", ParamType::CertificateOrString);
const BINARY: Param =
    Param::new("binary", ParamType::Bool).with_default(ConstantValue::Bool(false));

pub(crate) const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::asynchronous("openssl_x509_read", &[CERTIFICATE], read),
    FunctionEntry::asynchronous(
        "openssl_x509_export",
        &[
            CERTIFICATE,
            Param::new("output", ParamType::Mixed).by_ref(),
            Param::new("no_text", ParamType::Bool).with_default(ConstantValue::Bool(true)),
        ],
        export,
    ),
    FunctionEntry::asynchronous(
        "openssl_x509_fingerprint",
        &[
            CERTIFICATE,
            Param::new("digest_algo", ParamType::String)
                .with_default(ConstantValue::String("sha1")),
            BINARY,
        ],
        fingerprint,
    ),
    FunctionEntry::asynchronous(
        "openssl_x509_check_private_key",
        &[
            CERTIFICATE,
            Param::new("private_key", ParamType::Mixed).sensitive(),
        ],
        check_private,
    ),
    FunctionEntry::asynchronous(
        "openssl_x509_verify",
        &[CERTIFICATE, Param::new("public_key", ParamType::Mixed)],
        verify,
    ),
    FunctionEntry::sync(
        "openssl_x509_free",
        &[Param::new("certificate", ParamType::Class(CLASS))],
        free,
    ),
];

pub(crate) fn parse(bytes: &[u8]) -> Option<Certificate> {
    let bytes = bytes.trim_ascii_start();
    if bytes.starts_with(b"-----") {
        Certificate::from_pem(bytes).ok()
    } else {
        Certificate::from_der(bytes).ok()
    }
}

pub(crate) fn stored(object: &Object) -> Option<Rc<Certificate>> {
    object
        .native
        .borrow()
        .as_ref()?
        .downcast_ref::<Rc<Certificate>>()
        .cloned()
}

pub(crate) fn public_key(certificate: &Certificate) -> Option<Public> {
    let spki = certificate.tbs_certificate().subject_public_key_info();
    key_format::public_spki(&spki.to_der().ok()?)
}

async fn load(context: &mut dyn Context, value: &Value) -> Option<Rc<Certificate>> {
    match value {
        Value::Object(object) => stored(object),
        Value::InternedStr(string) => {
            let bytes = key_input::read_key_file(context, string.as_bytes()).await?;
            let certificate = parse(&bytes);
            if certificate.is_none() {
                errors::push(context, "error:0480006C:PEM routines::no start line");
            }
            certificate.map(Rc::new)
        }
        Value::String(string) => {
            let bytes = key_input::read_key_file(context, string.as_bytes()).await?;
            let certificate = parse(&bytes);
            if certificate.is_none() {
                errors::push(context, "error:0480006C:PEM routines::no start line");
            }
            certificate.map(Rc::new)
        }
        _ => None,
    }
}

fn free(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    context.deprecated(
        concat!(
            "Function openssl_x509_free() is deprecated since 8.0, ",
            "as OpenSSLCertificate objects are freed automatically",
        )
        .into(),
    );
    Ok(Value::Null)
}

fn read(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let Some(certificate) = load(context, args::value(&arguments, 0)).await else {
            warn_certificate(context, "openssl_x509_read");
            return Ok(Value::Bool(false));
        };
        let value = context.create_object(CLASS, PhpArray::new())?;
        if let Value::Object(object) = &value {
            *object.native.borrow_mut() = Some(Box::new(certificate));
        }
        Ok(value)
    })
}

fn export(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "openssl_x509_export";
        let Some(certificate) = load(context, args::value(&arguments, 0)).await else {
            warn_certificate(context, function);
            return Ok(Value::Bool(false));
        };
        if !args::bool(&arguments, 2).unwrap_or(true) {
            context.warning(format!(
                "{function}(): Phera does not support certificate text output"
            ));
            return Ok(Value::Bool(false));
        }
        let Ok(pem) = certificate.to_pem(LineEnding::LF) else {
            return Ok(Value::Bool(false));
        };
        if let Some(output) = args::reference(&arguments, 1) {
            *output.borrow_mut() = Value::from(pem);
        }
        Ok(Value::Bool(true))
    })
}

fn fingerprint(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "openssl_x509_fingerprint";
        let Some(certificate) = load(context, args::value(&arguments, 0)).await else {
            warn_certificate(context, function);
            return Ok(Value::Bool(false));
        };
        let digest = args::string(&arguments, 1).map_or(b"sha1".as_slice(), |name| name.as_bytes());
        let Some(digest) = Algorithm::resolve(context, function, digest) else {
            return Ok(Value::Bool(false));
        };
        let Ok(der) = certificate.to_der() else {
            return Ok(Value::Bool(false));
        };
        let hash = digest.hash(&der);
        if args::bool(&arguments, 2).unwrap_or_default() {
            Ok(Value::from(hash))
        } else {
            let hex: String = hash.iter().map(|byte| format!("{byte:02x}")).collect();
            Ok(Value::from(hex))
        }
    })
}

fn check_private(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let Some(certificate) = load(context, args::value(&arguments, 0)).await else {
            return Ok(Value::Bool(false));
        };
        let key = key_input::load(
            context,
            args::value(&arguments, 1),
            Role::Private,
            b"",
            "openssl_x509_check_private_key",
        )
        .await?;
        let Some(Key::Private(private)) = key.as_deref() else {
            return Ok(Value::Bool(false));
        };
        let certificate = public_key(&certificate).and_then(|key| key_format::public_pem(&key));
        let actual = key_format::public_pem(&private.public());
        let matches = match (&certificate, &actual) {
            (Some(certificate), Some(actual)) => {
                certificate.as_bytes().ct_eq(actual.as_bytes()).into()
            }
            _ => false,
        };
        Ok(Value::Bool(matches))
    })
}

fn verify(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let Some(certificate) = load(context, args::value(&arguments, 0)).await else {
            return Ok(Value::Int(-1));
        };
        let key = key_input::load(
            context,
            args::value(&arguments, 1),
            Role::Public,
            b"",
            "openssl_x509_verify",
        )
        .await?;
        let Some(Key::Public(public)) = key.as_deref() else {
            return Ok(Value::Int(-1));
        };
        Ok(Value::Int(
            signature_matches(&certificate, public).map_or(-1, i64::from),
        ))
    })
}

fn signature_matches(certificate: &Certificate, key: &Public) -> Option<bool> {
    let tbs = certificate.tbs_certificate();
    let algorithm = certificate.signature_algorithm();
    if algorithm != tbs.signature() {
        return Some(false);
    }
    let digest = certificate_digest(algorithm.oid, key)?;
    let der = tbs.to_der().ok()?;
    verify_certificate_bytes(key, digest, &der, certificate.signature().as_bytes()?)
}

fn certificate_digest(oid: ObjectIdentifier, key: &Public) -> Option<Option<Algorithm>> {
    use Algorithm::{Md5, Sha1, Sha224, Sha256, Sha384, Sha512, Sha512224, Sha512256};
    let is_ec = matches!(key, Public::P256(_) | Public::P384(_) | Public::P521(_));
    let oid = oid.to_string();
    let digest = if let Public::Rsa(_) = key {
        match oid.strip_prefix("1.2.840.113549.1.1.")? {
            "4" => Md5,
            "5" => Sha1,
            "11" => Sha256,
            "12" => Sha384,
            "13" => Sha512,
            "14" => Sha224,
            "15" => Sha512224,
            "16" => Sha512256,
            _ => return None,
        }
    } else if is_ec {
        match oid.as_str() {
            "1.2.840.10045.4.1" => Sha1,
            "1.2.840.10045.4.3.1" => Sha224,
            "1.2.840.10045.4.3.2" => Sha256,
            "1.2.840.10045.4.3.3" => Sha384,
            "1.2.840.10045.4.3.4" => Sha512,
            _ => return None,
        }
    } else if matches!(key, Public::Ed25519(_)) && oid == "1.3.101.112" {
        return Some(None);
    } else {
        return None;
    };
    Some(Some(digest))
}

fn warn_certificate(context: &mut dyn Context, function: &str) {
    context.warning(format!(
        "{function}(): X.509 Certificate cannot be retrieved"
    ));
}
