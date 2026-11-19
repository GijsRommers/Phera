//! `openssl_verify($data, $signature, $publicKey, 'sha256')` returns 1 on a match.

use ed25519_dalek::Signer;
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Value, args,
};
use rsa::signature::hazmat::{PrehashSigner, PrehashVerifier};

use crate::digest::Algorithm;
use crate::errors;
use crate::key::{Key, Private, Public};
use crate::key_input::{self, Role};
use crate::rsa_signatures::{Operation, Output, Padding};

const ALGORITHM: Param =
    Param::new("algorithm", ParamType::IntOrString).with_default(ConstantValue::Int(1));
const PADDING: Param = Param::new("padding", ParamType::Int).with_default(ConstantValue::Int(0));
const DATA: Param = Param::new("data", ParamType::String);
const SIGN: &str = "openssl_sign";
const VERIFY: &str = "openssl_verify";

pub(crate) const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::asynchronous(
        SIGN,
        &[
            DATA,
            Param::new("signature", ParamType::Mixed).by_ref(),
            Param::new("private_key", ParamType::Mixed),
            ALGORITHM,
            PADDING,
        ],
        sign,
    ),
    FunctionEntry::asynchronous(
        VERIFY,
        &[
            DATA,
            Param::new("signature", ParamType::String),
            Param::new("public_key", ParamType::Mixed),
            ALGORITHM,
            PADDING,
        ],
        verify,
    ),
];

#[derive(Clone, Copy)]
enum AlgorithmSelection {
    Default,
    Digest(Algorithm),
}

impl AlgorithmSelection {
    fn digest(self) -> Algorithm {
        match self {
            Self::Default => Algorithm::Sha256,
            Self::Digest(digest) => digest,
        }
    }
}

fn selection(context: &mut dyn Context, name: &str, value: &Value) -> Option<AlgorithmSelection> {
    let result = match value {
        Value::String(bytes) => Algorithm::resolve(context, name, bytes.as_bytes())?,
        Value::Int(0) => return Some(AlgorithmSelection::Default),
        Value::Int(1) | Value::Null => Algorithm::Sha1,
        Value::Int(2) => Algorithm::Md5,
        Value::Int(3) => Algorithm::resolve(context, name, b"md4")?,
        Value::Int(6) => Algorithm::Sha224,
        Value::Int(7) => Algorithm::Sha256,
        Value::Int(8) => Algorithm::Sha384,
        Value::Int(9) => Algorithm::Sha512,
        Value::Int(10) => Algorithm::Ripemd160,
        _ => {
            context.warning(format!("{name}(): Unknown digest algorithm"));
            return None;
        }
    };
    Some(AlgorithmSelection::Digest(result))
}

fn padding(
    context: &mut dyn Context,
    function: &str,
    rsa: bool,
    requested: i64,
) -> Option<Padding> {
    match (rsa, requested) {
        (_, 0) | (true, 1) => Some(Padding::Pkcs1),
        (true, 6) => Some(Padding::Pss),
        _ => {
            let message = if rsa {
                "Unknown padding type"
            } else {
                "Padding parameter is only supported for RSA keys"
            };
            context.warning(format!("{function}(): {message}"));
            None
        }
    }
}

fn sign(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let key = key_input::load(
            context,
            args::value(&arguments, 2),
            Role::Private,
            b"",
            SIGN,
        )
        .await?;
        let Some(Key::Private(key)) = key.as_deref() else {
            context.warning(format!(
                "{SIGN}(): Supplied key param cannot be coerced into a private key"
            ));
            return Ok(Value::Bool(false));
        };
        let Some(algorithm) = selection(context, SIGN, args::value(&arguments, 3)) else {
            return Ok(Value::Bool(false));
        };
        if matches!(key, Private::X25519(_)) {
            errors::push(
                context,
                concat!(
                    "error:03000096:digital envelope routines::",
                    "operation not supported for this keytype",
                ),
            );
            return Ok(Value::Bool(false));
        }
        if matches!(key, Private::Ed25519(_)) && !matches!(algorithm, AlgorithmSelection::Default) {
            errors::push(context, "error:1C80007A:Provider routines::invalid digest");
            return Ok(Value::Bool(false));
        }
        let Some(padding) = padding(
            context,
            SIGN,
            matches!(key, Private::Rsa(_)),
            args::int(&arguments, 4).unwrap_or_default(),
        ) else {
            return Ok(Value::Bool(false));
        };
        let Some(signature) = sign_bytes(key, algorithm, padding, args::bytes(&arguments, 0))
        else {
            return Ok(Value::Bool(false));
        };
        if let Some(output) = args::reference(&arguments, 1) {
            *output.borrow_mut() = Value::from(signature);
        }
        Ok(Value::Bool(true))
    })
}

fn verify(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let Some(algorithm) = selection(context, VERIFY, args::value(&arguments, 3)) else {
            return Ok(Value::Bool(false));
        };
        let key = key_input::load(
            context,
            args::value(&arguments, 2),
            Role::Public,
            b"",
            VERIFY,
        )
        .await?;
        let Some(Key::Public(key)) = key.as_deref() else {
            context.warning(format!(
                "{VERIFY}(): Supplied key param cannot be coerced into a public key"
            ));
            return Ok(Value::Bool(false));
        };
        if matches!(key, Public::X25519(_)) {
            errors::push(
                context,
                concat!(
                    "error:03000096:digital envelope routines::",
                    "operation not supported for this keytype",
                ),
            );
            return Ok(Value::Int(-1));
        }
        if matches!(key, Public::Ed25519(_)) && !matches!(algorithm, AlgorithmSelection::Default) {
            errors::push(context, "error:1C80007A:Provider routines::invalid digest");
            return Ok(Value::Int(-1));
        }
        let Some(padding) = padding(
            context,
            VERIFY,
            matches!(key, Public::Rsa(_)),
            args::int(&arguments, 4).unwrap_or_default(),
        ) else {
            return Ok(Value::Int(-1));
        };
        Ok(Value::Int(
            verify_bytes(
                key,
                algorithm,
                padding,
                args::bytes(&arguments, 0),
                args::bytes(&arguments, 1),
            )
            .map_or(-1, i64::from),
        ))
    })
}

fn sign_bytes(
    key: &Private,
    algorithm: AlgorithmSelection,
    padding: Padding,
    data: &[u8],
) -> Option<Vec<u8>> {
    match key {
        Private::Rsa(key) => match Operation::Sign(key).apply(algorithm.digest(), padding, data)? {
            Output::Signature(signature) => Some(signature),
            Output::Verified(_) => None,
        },
        Private::P256(key) => {
            let signer = p256::ecdsa::SigningKey::from_slice(&key.to_bytes()).ok()?;
            let signature: p256::ecdsa::DerSignature =
                signer.sign_prehash(&algorithm.digest().hash(data)).ok()?;
            Some(signature.as_bytes().to_vec())
        }
        Private::P384(key) => {
            let signer = p384::ecdsa::SigningKey::from_slice(&key.to_bytes()).ok()?;
            let signature: p384::ecdsa::DerSignature =
                signer.sign_prehash(&algorithm.digest().hash(data)).ok()?;
            Some(signature.as_bytes().to_vec())
        }
        Private::P521(key) => {
            let signer = p521::ecdsa::SigningKey::from_slice(&key.to_bytes()).ok()?;
            let signature: p521::ecdsa::DerSignature =
                signer.sign_prehash(&algorithm.digest().hash(data)).ok()?;
            Some(signature.as_bytes().to_vec())
        }
        Private::Ed25519(key) => Some(key.try_sign(data).ok()?.to_bytes().to_vec()),
        Private::X25519(_) => None,
    }
}

fn verify_bytes(
    key: &Public,
    algorithm: AlgorithmSelection,
    padding: Padding,
    data: &[u8],
    signature: &[u8],
) -> Option<bool> {
    match key {
        Public::Rsa(key) => {
            match Operation::Verify(key, signature).apply(algorithm.digest(), padding, data)? {
                Output::Verified(valid) => Some(valid),
                Output::Signature(_) => None,
            }
        }
        Public::P256(key) => {
            let signer = p256::ecdsa::VerifyingKey::from(*key);
            let signature = p256::ecdsa::Signature::from_der(signature).ok()?;
            Some(
                signer
                    .verify_prehash(&algorithm.digest().hash(data), &signature)
                    .is_ok(),
            )
        }
        Public::P384(key) => {
            let signer = p384::ecdsa::VerifyingKey::from(*key);
            let signature = p384::ecdsa::Signature::from_der(signature).ok()?;
            Some(
                signer
                    .verify_prehash(&algorithm.digest().hash(data), &signature)
                    .is_ok(),
            )
        }
        Public::P521(key) => {
            let signer = p521::ecdsa::VerifyingKey::from(*key);
            let signature = p521::ecdsa::Signature::from_der(signature).ok()?;
            Some(
                signer
                    .verify_prehash(&algorithm.digest().hash(data), &signature)
                    .is_ok(),
            )
        }
        Public::Ed25519(key) => Some(
            ed25519_dalek::Signature::from_slice(signature)
                .is_ok_and(|signature| key.verify_strict(data, &signature).is_ok()),
        ),
        Public::X25519(_) => None,
    }
}
