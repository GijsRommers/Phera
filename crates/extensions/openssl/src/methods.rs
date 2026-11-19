//! `openssl_cipher_iv_length('aes-256-gcm')` returns 12.

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Value, args,
};

pub(crate) const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::sync("openssl_cipher_iv_length", CIPHER_NAME, iv_length),
    FunctionEntry::sync("openssl_cipher_key_length", CIPHER_NAME, key_length),
    FunctionEntry::sync(
        "openssl_get_cipher_methods",
        &[Param::new("aliases", ParamType::Bool).with_default(ConstantValue::Bool(false))],
        cipher_methods,
    ),
];

const CIPHER_NAME: &[Param] = &[Param::new("cipher_algo", ParamType::String)];
const AES_MODES: &[(&str, Mode)] = &[
    ("cbc", Mode::Cbc),
    ("ccm", Mode::Ccm),
    ("cfb", Mode::Cfb),
    ("ctr", Mode::Ctr),
    ("ecb", Mode::Ecb),
    ("gcm", Mode::Gcm),
    ("ofb", Mode::Ofb),
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Cbc,
    Ccm,
    Cfb,
    Ctr,
    Ecb,
    Gcm,
    Ofb,
    ChaCha,
}

#[derive(Clone, Copy)]
pub(crate) enum KeySize {
    Bits128,
    Bits192,
    Bits256,
}

#[derive(Clone, Copy)]
pub(crate) struct Method {
    pub(crate) mode: Mode,
    pub(crate) key: KeySize,
}

impl Method {
    pub(crate) fn iv_length(self) -> usize {
        match self.mode {
            Mode::Ecb => 0,
            Mode::Gcm | Mode::Ccm | Mode::ChaCha => 12,
            _ => 16,
        }
    }

    pub(crate) fn key_length(self) -> usize {
        match self.key {
            KeySize::Bits128 => 16,
            KeySize::Bits192 => 24,
            KeySize::Bits256 => 32,
        }
    }
}

impl Mode {
    pub(crate) fn authenticated(self) -> bool {
        matches!(self, Self::Ccm | Self::Gcm | Self::ChaCha)
    }
}

pub(crate) fn c_name(name: &[u8]) -> Option<String> {
    let name = name.split(|byte| *byte == 0).next()?;
    Some(std::str::from_utf8(name).ok()?.to_ascii_lowercase())
}

pub(crate) fn resolve(context: &mut dyn Context, function: &str, bytes: &[u8]) -> Option<Method> {
    let name = c_name(bytes).unwrap_or_default();
    if let Some(method) = lookup(&name)
        .or_else(|| lookup_alias(&name))
        .or_else(|| lookup_oid(&name))
    {
        return Some(method);
    }
    if known_unsupported(&name) {
        let name = String::from_utf8_lossy(bytes);
        context.warning(format!(
            "{function}(): Phera does not support cipher algorithm {name}"
        ));
    } else {
        context.warning(format!("{function}(): Unknown cipher algorithm"));
    }
    None
}

fn lookup(name: &str) -> Option<Method> {
    if name == "chacha20-poly1305" {
        return Some(Method {
            key: KeySize::Bits256,
            mode: Mode::ChaCha,
        });
    }
    let (bits, suffix) = match name.split_once('-')? {
        ("aes", rest) => rest.split_once('-')?,
        ("id", rest) => rest.strip_prefix("aes")?.split_once('-')?,
        _ => return None,
    };
    let key = key_size(bits)?;
    let mode = AES_MODES.iter().find(|(name, _)| *name == suffix)?.1;
    if name.starts_with("id-") && !matches!(mode, Mode::Gcm | Mode::Ccm) {
        return None;
    }
    Some(Method { key, mode })
}

fn key_size(bits: &str) -> Option<KeySize> {
    match bits {
        "128" => Some(KeySize::Bits128),
        "192" => Some(KeySize::Bits192),
        "256" => Some(KeySize::Bits256),
        _ => None,
    }
}

fn lookup_alias(name: &str) -> Option<Method> {
    let bits = name.strip_prefix("aes")?;
    Some(Method {
        key: key_size(bits)?,
        mode: Mode::Cbc,
    })
}

fn known_unsupported(name: &str) -> bool {
    let family = name.split('-').next().unwrap_or_default();
    if matches!(family, "aes" | "aria" | "camellia") {
        return name.split_once('-').is_some_and(|(_, tail)| {
            tail.split_once('-').is_some_and(|(bits, mode)| {
                key_size(bits).is_some()
                    && matches!(
                        mode,
                        "cbc"
                            | "ccm"
                            | "cfb"
                            | "cfb1"
                            | "cfb8"
                            | "ctr"
                            | "ecb"
                            | "gcm"
                            | "ofb"
                            | "ocb"
                            | "siv"
                            | "gcm-siv"
                            | "xts"
                            | "cbc-cts"
                            | "wrap"
                            | "wrap-pad"
                            | "wrap-inv"
                            | "wrap-pad-inv"
                            | "cbc-hmac-sha1"
                            | "cbc-hmac-sha256"
                            | "cbc-hmac-sha1-etm"
                            | "cbc-hmac-sha256-etm"
                            | "cbc-hmac-sha512-etm"
                    )
            })
        });
    }
    matches!(
        family,
        "rc2" | "rc4" | "bf" | "blowfish" | "des" | "des3" | "seed" | "sm4"
    ) || matches!(
        name,
        "chacha20"
            | "null"
            | "aria128"
            | "aria192"
            | "aria256"
            | "camellia128"
            | "camellia192"
            | "camellia256"
    )
}

fn query_length(
    context: &mut dyn Context,
    arguments: &[Value],
    function: &str,
    length: impl Fn(Method) -> usize,
) -> NativeResult {
    let name = args::bytes(arguments, 0);
    if name.is_empty() {
        return Err(empty_cipher(function, 1));
    }
    Ok(resolve(context, function, name)
        .map_or(Value::Bool(false), |m| Value::Int(length(m) as i64)))
}

pub(crate) fn empty_cipher(function: &str, index: usize) -> NativeError {
    NativeError::value_error(format!(
        "{function}(): Argument #{index} ($cipher_algo) must not be empty"
    ))
}

fn iv_length(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    query_length(
        context,
        arguments,
        "openssl_cipher_iv_length",
        Method::iv_length,
    )
}

fn key_length(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    query_length(
        context,
        arguments,
        "openssl_cipher_key_length",
        Method::key_length,
    )
}

fn cipher_methods(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut names = Vec::new();
    let aliases = args::bool(arguments, 0).unwrap_or_default();
    for (bits, oid_suffixes) in [
        (128, [2, 7, 4, 0, 1, 6, 3]),
        (192, [22, 27, 24, 0, 21, 26, 23]),
        (256, [42, 47, 44, 0, 41, 46, 43]),
    ] {
        for ((mode, _), oid_suffix) in AES_MODES.iter().zip(oid_suffixes) {
            names.push(format!("aes-{bits}-{mode}"));
            if aliases && oid_suffix != 0 {
                names.push(format!("2.16.840.1.101.3.4.1.{oid_suffix}"));
            }
        }
        if aliases {
            names.push(format!("aes{bits}"));
            names.push(format!("id-aes{bits}-ccm"));
            names.push(format!("id-aes{bits}-gcm"));
        }
    }
    names.push("chacha20-poly1305".to_owned());
    names.sort_unstable();
    let mut output = PhpArray::new();
    for name in names {
        output.push(Value::from(name))?;
    }
    Ok(Value::from(output))
}

fn lookup_oid(name: &str) -> Option<Method> {
    let suffix: u8 = name.strip_prefix("2.16.840.1.101.3.4.1.")?.parse().ok()?;
    let key = match suffix / 20 {
        0 => KeySize::Bits128,
        1 => KeySize::Bits192,
        2 => KeySize::Bits256,
        _ => return None,
    };
    let mode = match suffix % 20 {
        1 => Mode::Ecb,
        2 => Mode::Cbc,
        3 => Mode::Ofb,
        4 => Mode::Cfb,
        6 => Mode::Gcm,
        7 => Mode::Ccm,
        _ => return None,
    };
    Some(Method { key, mode })
}
