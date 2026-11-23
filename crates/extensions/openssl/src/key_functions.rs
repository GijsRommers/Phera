//! `openssl_pkey_export_to_file($key, 'private.pem', 'passphrase')` writes PEM.

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, PhpArray, Value, args,
};
use rsa::Generate;

use crate::key::{self, CLASS, Key, OsRandom, Private};
use crate::key_details;
use crate::key_format::{self, Encryption};
use crate::key_input::{self, Role};

const OPTIONS: Param = Param::new("options", ParamType::Array)
    .nullable()
    .with_default(ConstantValue::Null);
const PASSPHRASE: Param = Param::new("passphrase", ParamType::String)
    .sensitive()
    .nullable()
    .with_default(ConstantValue::Null);
const PATH: Param = Param::new("output_filename", ParamType::String);
const KEY: Param = Param::new("key", ParamType::Mixed).sensitive();
const PRIVATE: Param = Param::new("private_key", ParamType::Mixed).sensitive();
const PUBLIC: Param = Param::new("public_key", ParamType::Mixed);

pub(crate) const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::asynchronous("openssl_pkey_new", &[OPTIONS], generate),
    FunctionEntry::asynchronous(
        "openssl_pkey_get_private",
        &[PRIVATE, PASSPHRASE],
        get_private,
    ),
    FunctionEntry::asynchronous(
        "openssl_get_privatekey",
        &[PRIVATE, PASSPHRASE],
        get_private,
    ),
    FunctionEntry::asynchronous("openssl_pkey_get_public", &[PUBLIC], get_public),
    FunctionEntry::asynchronous("openssl_get_publickey", &[PUBLIC], get_public),
    FunctionEntry::sync(
        "openssl_pkey_get_details",
        &[Param::new("key", ParamType::Class(CLASS))],
        key_details::details,
    ),
    FunctionEntry::asynchronous(
        "openssl_pkey_export",
        &[
            KEY,
            Param::new("output", ParamType::Mixed).by_ref(),
            PASSPHRASE,
            OPTIONS,
        ],
        export,
    ),
    FunctionEntry::asynchronous(
        "openssl_pkey_export_to_file",
        &[KEY, PATH, PASSPHRASE, OPTIONS],
        export_to_file,
    ),
    FunctionEntry::deprecated_sync(
        "openssl_pkey_free",
        &[Param::new("key", ParamType::Class(CLASS))],
        free,
    ),
    FunctionEntry::sync(
        "openssl_free_key",
        &[Param::new("key", ParamType::Class(CLASS))],
        free_alias,
    ),
];

fn free(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::Null)
}

fn free_alias(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    deprecate_free(context, "openssl_free_key")
}

fn deprecate_free(context: &mut dyn Context, function: &str) -> NativeResult {
    context.deprecated(format!(
        "Function {function}() is deprecated since 8.0, \
         as OpenSSLAsymmetricKey objects are freed automatically"
    ));
    Ok(Value::Null)
}

enum GenerateKey {
    Rsa(usize),
    P256,
    P384,
    P521,
    X25519,
    Ed25519,
}

impl GenerateKey {
    fn new(context: &mut dyn Context, options: Option<&Rc<PhpArray>>) -> Option<Self> {
        let value = |name| {
            options?
                .get(&ArrayKey::str(name))
                .map(|value| value.deref().clone())
        };
        if ["rsa", "dsa", "dh", "ec", "ed25519", "x25519"]
            .iter()
            .any(|name| matches!(value(*name), Some(Value::Array(_))))
        {
            return None;
        }
        let bits = match value("private_key_bits") {
            Some(Value::Int(bits)) => bits,
            _ => 2048,
        };
        let kind = match value("private_key_type") {
            Some(Value::Int(kind)) => kind,
            _ => 0,
        };
        let method = match kind {
            0 if bits < 384 => {
                context.warning(format!(
                    "openssl_pkey_new(): Private key length must be at least 384 bits, \
                     configured to {bits}"
                ));
                return None;
            }
            0 if (2048..=4096).contains(&bits) => Self::Rsa(bits as usize),
            0 => {
                context.warning(format!(
                    "openssl_pkey_new(): Phera does not support RSA-{bits}"
                ));
                return None;
            }
            3 => {
                let Some(Value::String(curve)) = value("curve_name") else {
                    context.warning(
                        concat!(
                            "openssl_pkey_new(): Missing configuration value: ",
                            "\"curve_name\" not set"
                        )
                        .into(),
                    );
                    return None;
                };
                match curve.as_bytes() {
                    b"prime256v1" => Self::P256,
                    b"secp384r1" => Self::P384,
                    b"secp521r1" => Self::P521,
                    _ => {
                        let curve = curve.to_string_lossy();
                        context.warning(format!(
                            "openssl_pkey_new(): Unknown elliptic curve (short) name {curve}"
                        ));
                        return None;
                    }
                }
            }
            4 => Self::X25519,
            5 => Self::Ed25519,
            _ => {
                context.warning("openssl_pkey_new(): Unsupported private key type".into());
                return None;
            }
        };
        Some(method)
    }

    fn private_key(self) -> Option<Private> {
        let mut rng = OsRandom;
        match self {
            Self::Rsa(bits) => rsa::PrivateKey::new(&mut rng, bits)
                .ok()
                .map(Box::new)
                .map(Private::Rsa),
            Self::P256 => p256::SecretKey::try_generate_from_rng(&mut rng)
                .ok()
                .map(Private::P256),
            Self::P384 => p384::SecretKey::try_generate_from_rng(&mut rng)
                .ok()
                .map(Private::P384),
            Self::P521 => p521::SecretKey::try_generate_from_rng(&mut rng)
                .ok()
                .map(Private::P521),
            Self::X25519 | Self::Ed25519 => {
                let mut seed = [0; 32];
                getrandom::fill(&mut seed).ok()?;
                match self {
                    Self::Ed25519 => Some(Private::Ed25519(ed25519_dalek::SigningKey::from_bytes(
                        &seed,
                    ))),
                    _ => Some(Private::X25519(seed.into())),
                }
            }
        }
    }
}

fn generate(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let Some(method) = GenerateKey::new(context, args::array(&arguments, 0)) else {
            return Ok(Value::Bool(false));
        };
        let key = if matches!(method, GenerateKey::Rsa(_)) {
            tokio::task::spawn_blocking(move || method.private_key())
                .await
                .ok()
                .flatten()
        } else {
            method.private_key()
        };
        match key {
            Some(key) => key::make(context, Rc::new(Key::Private(key))),
            None => Ok(Value::Bool(false)),
        }
    })
}

fn get_private(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let key = key_input::load(
            context,
            args::value(&arguments, 0),
            Role::Private,
            args::bytes(&arguments, 1),
            "openssl_pkey_get_private",
        )
        .await?;
        match key {
            Some(key) => key::make(context, key),
            None => Ok(Value::Bool(false)),
        }
    })
}

fn get_public(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let key = key_input::load(
            context,
            args::value(&arguments, 0),
            Role::Public,
            b"",
            "openssl_pkey_get_public",
        )
        .await?;
        match key {
            Some(key) => key::make(context, key),
            None => Ok(Value::Bool(false)),
        }
    })
}

fn export(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let Some(pem) = export_pem(context, &arguments, "openssl_pkey_export").await? else {
            return Ok(Value::Bool(false));
        };
        if let Some(output) = args::reference(&arguments, 1) {
            *output.borrow_mut() = Value::from(pem);
        }
        Ok(Value::Bool(true))
    })
}

fn export_to_file(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "openssl_pkey_export_to_file";
        let bytes = args::bytes(&arguments, 1);
        let Some(path) = key_input::path_from_bytes(bytes) else {
            return Err(NativeError::value_error(format!(
                "{function}(): Argument #2 ($output_filename) must not contain any null bytes"
            )));
        };
        let Ok(path) = key_input::working_path(context, path) else {
            return Ok(Value::Bool(false));
        };
        let maximum = if cfg!(target_os = "macos") {
            1024
        } else {
            4096
        };
        if bytes.is_empty() || bytes.len() >= maximum || path.as_os_str().len() >= maximum {
            context.warning(format!(
                "{function}(output_filename): must be a valid file path"
            ));
            return Ok(Value::Bool(false));
        }
        let Some(pem) = export_pem(context, &arguments, function).await? else {
            return Ok(Value::Bool(false));
        };
        Ok(Value::Bool(tokio::fs::write(path, pem).await.is_ok()))
    })
}

async fn export_pem(
    context: &mut dyn Context,
    arguments: &[Value],
    function: &str,
) -> Result<Option<Vec<u8>>, NativeError> {
    let password = args::string(arguments, 2).map(|string| string.as_bytes());
    let key = key_input::load(
        context,
        args::value(arguments, 0),
        Role::Private,
        password.unwrap_or_default(),
        function,
    )
    .await?;
    let Some(Key::Private(key)) = key.as_deref() else {
        context.warning(format!("{function}(): Cannot get key from parameter 1"));
        return Ok(None);
    };
    let option = |name| args::array(arguments, 3)?.get(&ArrayKey::str(name));
    let encryption = if let Some(password) =
        password.filter(|_| !matches!(option("encrypt_key"), Some(Value::Bool(false))))
    {
        let cipher = match option("encrypt_key_cipher") {
            Some(Value::Int(5)) => Encryption::Aes128,
            Some(Value::Int(6)) => Encryption::Aes192,
            Some(Value::Int(7)) => Encryption::Aes256,
            None | Some(Value::Int(4)) => Encryption::TripleDes,
            Some(_) => {
                context.warning(format!(
                    "{function}(): Phera does not support this key cipher"
                ));
                return Ok(None);
            }
        };
        Some((password, cipher))
    } else {
        None
    };
    Ok(key_format::export(key, encryption))
}
