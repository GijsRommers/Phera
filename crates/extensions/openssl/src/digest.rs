//! `openssl_digest('hello', 'sha256')` returns hex.
//! `openssl_pbkdf2('password', 'salt', 32, 1000, 'sha256')` returns bytes.

use digest::Digest;
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Value, args,
};

use crate::{errors, methods::c_name};

pub(crate) const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::sync(
        "openssl_digest",
        &[
            Param::new("data", ParamType::String),
            Param::new("digest_algo", ParamType::String),
            Param::new("binary", ParamType::Bool).with_default(ConstantValue::Bool(false)),
        ],
        digest,
    ),
    FunctionEntry::sync(
        "openssl_pbkdf2",
        &[
            Param::new("password", ParamType::String),
            Param::new("salt", ParamType::String),
            Param::new("key_length", ParamType::Int),
            Param::new("iterations", ParamType::Int),
            Param::new("digest_algo", ParamType::String)
                .with_default(ConstantValue::String("sha1")),
        ],
        pbkdf2,
    ),
    FunctionEntry::sync(
        "openssl_get_md_methods",
        &[Param::new("aliases", ParamType::Bool).with_default(ConstantValue::Bool(false))],
        methods,
    ),
];

#[derive(Clone, Copy)]
pub(crate) enum Algorithm {
    Md5,
    Ripemd160,
    Sha1,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
    Sha512224,
    Sha512256,
    Sha3_224,
    Sha3_256,
    Sha3_384,
    Sha3_512,
}

const ALGORITHMS: &[(&str, Algorithm, &[&str])] = &[
    (
        "md5",
        Algorithm::Md5,
        &["RSA-MD5", "md5WithRSAEncryption", "ssl3-md5"],
    ),
    (
        "ripemd160",
        Algorithm::Ripemd160,
        &["RSA-RIPEMD160", "ripemd", "ripemd160WithRSA", "rmd160"],
    ),
    (
        "sha1",
        Algorithm::Sha1,
        &[
            "RSA-SHA1",
            "RSA-SHA1-2",
            "sha1WithRSAEncryption",
            "ssl3-sha1",
        ],
    ),
    (
        "sha224",
        Algorithm::Sha224,
        &["RSA-SHA224", "sha224WithRSAEncryption"],
    ),
    (
        "sha256",
        Algorithm::Sha256,
        &["RSA-SHA256", "sha256WithRSAEncryption"],
    ),
    (
        "sha3-224",
        Algorithm::Sha3_224,
        &["RSA-SHA3-224", "id-rsassa-pkcs1-v1_5-with-sha3-224"],
    ),
    (
        "sha3-256",
        Algorithm::Sha3_256,
        &["RSA-SHA3-256", "id-rsassa-pkcs1-v1_5-with-sha3-256"],
    ),
    (
        "sha3-384",
        Algorithm::Sha3_384,
        &["RSA-SHA3-384", "id-rsassa-pkcs1-v1_5-with-sha3-384"],
    ),
    (
        "sha3-512",
        Algorithm::Sha3_512,
        &["RSA-SHA3-512", "id-rsassa-pkcs1-v1_5-with-sha3-512"],
    ),
    (
        "sha384",
        Algorithm::Sha384,
        &["RSA-SHA384", "sha384WithRSAEncryption"],
    ),
    (
        "sha512",
        Algorithm::Sha512,
        &["RSA-SHA512", "sha512WithRSAEncryption"],
    ),
    (
        "sha512-224",
        Algorithm::Sha512224,
        &["RSA-SHA512/224", "sha512-224WithRSAEncryption"],
    ),
    (
        "sha512-256",
        Algorithm::Sha512256,
        &["RSA-SHA512/256", "sha512-256WithRSAEncryption"],
    ),
];

impl Algorithm {
    pub(crate) fn resolve(context: &mut dyn Context, function: &str, bytes: &[u8]) -> Option<Self> {
        let name = c_name(bytes).unwrap_or_default();
        for (canonical, algorithm, aliases) in ALGORITHMS {
            if name == *canonical || aliases.iter().any(|alias| name.eq_ignore_ascii_case(alias)) {
                return Some(*algorithm);
            }
        }
        match name.as_str() {
            "md2"
            | "md4"
            | "mdc2"
            | "sha"
            | "sha0"
            | "sha-0"
            | "rsa-md2"
            | "rsa-md4"
            | "md2withrsaencryption"
            | "md4withrsaencryption"
            | "blake2b512"
            | "blake2s256"
            | "whirlpool"
            | "shake128"
            | "shake256"
            | "sm3"
            | "rsa-sm3"
            | "sm3withrsaencryption"
            | "md5-sha1" => {
                let name = String::from_utf8_lossy(bytes);
                context.warning(format!(
                    "{function}(): Phera does not support digest algorithm {name}"
                ));
            }
            _ => context.warning(format!("{function}(): Unknown digest algorithm")),
        }
        None
    }

    pub(crate) fn hash(self, data: &[u8]) -> Vec<u8> {
        match self {
            Self::Md5 => md5::Md5::digest(data).to_vec(),
            Self::Ripemd160 => ripemd::Ripemd160::digest(data).to_vec(),
            Self::Sha1 => sha1::Sha1::digest(data).to_vec(),
            Self::Sha224 => sha2::Sha224::digest(data).to_vec(),
            Self::Sha256 => sha2::Sha256::digest(data).to_vec(),
            Self::Sha384 => sha2::Sha384::digest(data).to_vec(),
            Self::Sha512 => sha2::Sha512::digest(data).to_vec(),
            Self::Sha512224 => sha2::Sha512_224::digest(data).to_vec(),
            Self::Sha512256 => sha2::Sha512_256::digest(data).to_vec(),
            Self::Sha3_224 => sha3::Sha3_224::digest(data).to_vec(),
            Self::Sha3_256 => sha3::Sha3_256::digest(data).to_vec(),
            Self::Sha3_384 => sha3::Sha3_384::digest(data).to_vec(),
            Self::Sha3_512 => sha3::Sha3_512::digest(data).to_vec(),
        }
    }

    fn derive(self, password: &[u8], salt: &[u8], rounds: u32, output: &mut [u8]) {
        use pbkdf2::pbkdf2_hmac;
        match self {
            Self::Md5 => pbkdf2_hmac::<md5::Md5>(password, salt, rounds, output),
            Self::Ripemd160 => pbkdf2_hmac::<ripemd::Ripemd160>(password, salt, rounds, output),
            Self::Sha1 => pbkdf2_hmac::<sha1::Sha1>(password, salt, rounds, output),
            Self::Sha224 => pbkdf2_hmac::<sha2::Sha224>(password, salt, rounds, output),
            Self::Sha256 => pbkdf2_hmac::<sha2::Sha256>(password, salt, rounds, output),
            Self::Sha384 => pbkdf2_hmac::<sha2::Sha384>(password, salt, rounds, output),
            Self::Sha512 => pbkdf2_hmac::<sha2::Sha512>(password, salt, rounds, output),
            Self::Sha512224 => pbkdf2_hmac::<sha2::Sha512_224>(password, salt, rounds, output),
            Self::Sha512256 => pbkdf2_hmac::<sha2::Sha512_256>(password, salt, rounds, output),
            Self::Sha3_224 => pbkdf2_hmac::<sha3::Sha3_224>(password, salt, rounds, output),
            Self::Sha3_256 => pbkdf2_hmac::<sha3::Sha3_256>(password, salt, rounds, output),
            Self::Sha3_384 => pbkdf2_hmac::<sha3::Sha3_384>(password, salt, rounds, output),
            Self::Sha3_512 => pbkdf2_hmac::<sha3::Sha3_512>(password, salt, rounds, output),
        }
    }
}

fn digest(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let method = args::bytes(arguments, 1);
    let Some(algorithm) = Algorithm::resolve(context, "openssl_digest", method) else {
        return Ok(Value::Bool(false));
    };
    let digest = algorithm.hash(args::bytes(arguments, 0));
    if args::bool(arguments, 2).unwrap_or_default() {
        Ok(Value::from(digest))
    } else {
        let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        Ok(Value::from(hex))
    }
}

fn lengths(arguments: &[Value]) -> Result<(usize, i32), NativeError> {
    let length = i32::try_from(args::int(arguments, 2).unwrap_or_default()).map_err(|_| {
        NativeError::value_error("openssl_pbkdf2(): Argument #3 ($key_length) key is too long")
    })?;
    let iterations = i32::try_from(args::int(arguments, 3).unwrap_or_default()).map_err(|_| {
        NativeError::value_error(
            "openssl_pbkdf2(): Argument #4 ($iterations) iterations is too long",
        )
    })?;
    if length <= 0 {
        return Err(NativeError::value_error(
            "openssl_pbkdf2(): Argument #3 ($key_length) must be greater than 0",
        ));
    }
    Ok((length as usize, iterations))
}

fn pbkdf2(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let (length, rounds) = lengths(arguments)?;
    let method = args::bytes(arguments, 4);
    let method = if method.is_empty() { b"sha1" } else { method };
    let Some(algorithm) = Algorithm::resolve(context, "openssl_pbkdf2", method) else {
        return Ok(Value::Bool(false));
    };
    if rounds <= 0 {
        let error = if rounds == 0 {
            "error:1C80007B:Provider routines::invalid iteration count"
        } else {
            concat!(
                "error:0780007D:common libcrypto routines::",
                "param unsigned integer negative value unsupported"
            )
        };
        errors::push(context, error);
        return Ok(Value::Bool(false));
    }
    let mut output = Vec::new();
    if output.try_reserve_exact(length).is_err() {
        return Ok(Value::Bool(false));
    }
    output.resize(length, 0);
    algorithm.derive(
        args::bytes(arguments, 0),
        args::bytes(arguments, 1),
        rounds as u32,
        &mut output,
    );
    Ok(Value::from(output))
}

fn methods(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let include_aliases = args::bool(arguments, 0).unwrap_or_default();
    let mut names = Vec::new();
    for &(name, _, aliases) in ALGORITHMS {
        names.push(name);
        if include_aliases {
            names.extend_from_slice(aliases);
        }
    }
    names.sort_unstable();
    let mut output = PhpArray::new();
    for name in names {
        output.push(Value::from(name))?;
    }
    Ok(Value::from(output))
}
