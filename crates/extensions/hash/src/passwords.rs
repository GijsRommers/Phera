//! `password_hash` and `password_verify` run CPU work off the Tokio request worker.
use crate::{
    password_info,
    password_options::{Algorithm, Options},
};
use argon2::{
    Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version,
    password_hash::SaltString,
};
use phera_extension_api::{ArrayKey, Context, LocalFuture, NativeError, NativeResult, Value, args};
use subtle::ConstantTimeEq;

pub fn hash(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let algorithm = Algorithm::parse(arguments.get(1)).ok_or_else(|| {
            NativeError::value_error(
                "password_hash(): Argument #2 ($algo) must be a valid password hashing algorithm",
            )
        })?;
        let options = Options::read(context, &arguments)?;
        let password = args::bytes(&arguments, 0).to_vec();
        options.validate(algorithm, &password)?;
        if args::array(&arguments, 2).is_some_and(|a| a.contains_key(&ArrayKey::str("salt"))) {
            context.warning("password_hash(): The \"salt\" option has been ignored, since providing a custom salt is no longer supported".into());
        }
        // Only owned Rust bytes and options cross the Send boundary; no PHP Rc values do.
        let result =
            tokio::task::spawn_blocking(move || hash_password(algorithm, &options, &password))
                .await
                .map_err(|e| NativeError::error(e.to_string()))?
                .map_err(NativeError::value_error)?;
        Ok(Value::from(result))
    })
}
fn hash_password(
    algorithm: Algorithm,
    options: &Options,
    password: &[u8],
) -> Result<String, String> {
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|e| e.to_string())?;
    if algorithm == Algorithm::Bcrypt {
        return bcrypt::hash_with_salt(password, options.cost as u32, salt)
            .map(|parts| parts.format_for_version(bcrypt::Version::TwoY))
            .map_err(|e| e.to_string());
    }
    let kind = if algorithm == Algorithm::Argon2i {
        argon2::Algorithm::Argon2i
    } else {
        argon2::Algorithm::Argon2id
    };
    let params = Params::new(
        options.memory as u32,
        options.time as u32,
        options.threads as u32,
        Some(32),
    )
    .map_err(|e| e.to_string())?;
    let salt = SaltString::encode_b64(&salt).map_err(|e| e.to_string())?;
    Argon2::new(kind, Version::V0x13, params)
        .hash_password(password, &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}
pub fn verify(_: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let password = args::bytes(&arguments, 0).to_vec();
        let hash = args::bytes(&arguments, 1).to_vec();
        let result = tokio::task::spawn_blocking(move || {
            let Ok(hash_text) = std::str::from_utf8(&hash) else {
                return false;
            };
            if matches!(
                password_info::identify(&hash),
                Some(Algorithm::Argon2i | Algorithm::Argon2id)
            ) {
                return PasswordHash::new(hash_text).is_ok_and(|parsed| {
                    Argon2::default()
                        .verify_password(&password, &parsed)
                        .is_ok()
                });
            }
            if hash.len() < 13 {
                return false;
            }
            let computed = crypt_bytes(&password, &hash);
            bool::from(computed.as_bytes().ct_eq(&hash))
        })
        .await
        .map_err(|e| NativeError::error(e.to_string()))?;
        Ok(Value::Bool(result))
    })
}
pub fn crypt(_: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let password = args::bytes(&arguments, 0).to_vec();
        let salt = args::bytes(&arguments, 1).to_vec();
        let result = tokio::task::spawn_blocking(move || crypt_bytes(&password, &salt))
            .await
            .map_err(|e| NativeError::error(e.to_string()))?;
        Ok(Value::from(result))
    })
}
fn crypt_bytes(password: &[u8], salt: &[u8]) -> String {
    let password = password.split(|&b| b == 0).next().unwrap_or_default();
    let salt = salt.split(|&b| b == 0).next().unwrap_or_default();
    let failure = if salt.starts_with(b"*0") { "*1" } else { "*0" };
    let Ok(salt) = std::str::from_utf8(salt) else {
        return failure.into();
    };
    if salt.starts_with('$')
        && !["$1$", "$2a$", "$2b$", "$2y$", "$5$", "$6$"]
            .iter()
            .any(|prefix| salt.starts_with(prefix))
    {
        return failure.into();
    }
    pwhash::unix::crypt(password, salt).unwrap_or_else(|_| failure.into())
}
