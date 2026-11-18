//! Hashing and checking passwords, and `crypt()`:
//!
//! ```php
//! $hash = password_hash('secret', PASSWORD_DEFAULT);  // "$2y$12$..."
//! password_verify('secret', $hash);                   // true
//! crypt('secret', '$6$salt$');                         // SHA-512 crypt
//! ```
//!
//! These take tens of milliseconds by design, so the work runs on Tokio's
//! blocking threads and the request's worker thread serves other requests
//! meanwhile. Only owned bytes and options cross over; PHP values hold
//! `Rc`s and must stay on their thread.

use argon2::password_hash::SaltString;
use argon2::{Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version};
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType,
    Value, args,
};
use subtle::ConstantTimeEq;

use crate::off_thread;
use crate::password_info::identify;
use crate::password_options::{self, Algorithm, Options};

pub const FUNCTIONS: &[FunctionEntry] = &[PASSWORD_HASH, PASSWORD_VERIFY, CRYPT];

pub const STRING_CONSTANTS: &[(&str, &str)] = &[
    ("PASSWORD_DEFAULT", "2y"),
    ("PASSWORD_BCRYPT", "2y"),
    ("PASSWORD_ARGON2I", "argon2i"),
    ("PASSWORD_ARGON2ID", "argon2id"),
    ("PASSWORD_ARGON2_PROVIDER", "standard"),
];

pub const INT_CONSTANTS: &[(&str, i64)] = &[
    (
        "PASSWORD_BCRYPT_DEFAULT_COST",
        password_options::DEFAULT_COST,
    ),
    (
        "PASSWORD_ARGON2_DEFAULT_MEMORY_COST",
        password_options::DEFAULT_MEMORY_COST,
    ),
    (
        "PASSWORD_ARGON2_DEFAULT_TIME_COST",
        password_options::DEFAULT_TIME_COST,
    ),
    (
        "PASSWORD_ARGON2_DEFAULT_THREADS",
        password_options::DEFAULT_THREADS,
    ),
];

/// Both bcrypt and Argon2 hashes carry a 16-byte random salt.
const SALT_BYTES: usize = 16;

/// PHP's Argon2 hashes are 32 bytes long.
const ARGON2_OUTPUT_BYTES: usize = 32;

/// The shortest hash `crypt()` can produce, two salt characters and eleven
/// of DES output; anything shorter cannot verify.
const SHORTEST_CRYPT: usize = 13;

/// `password_hash(string $password, string|int|null $algo, array $options = []): string`
const PASSWORD_HASH: FunctionEntry = FunctionEntry::asynchronous(
    "password_hash",
    &[
        Param::new("password", ParamType::String),
        Param::new("algo", ParamType::IntOrString).nullable(),
        Param::new("options", ParamType::Array).optional(),
    ],
    password_hash,
);

fn password_hash(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let algorithm = Algorithm::parse(args::value(&arguments, 1)).ok_or_else(|| {
            NativeError::value_error(
                "password_hash(): Argument #2 ($algo) must be a valid password hashing algorithm",
            )
        })?;
        let password = args::bytes(&arguments, 0).to_vec();
        if algorithm == Algorithm::Bcrypt && password.contains(&0) {
            return Err(NativeError::value_error(
                "Bcrypt password must not contain null character",
            ));
        }
        let options = Options::read(context, &arguments, algorithm)?;
        options.validate(algorithm)?;
        if args::array(&arguments, 2)
            .is_some_and(|options| options.contains_key(&ArrayKey::str("salt")))
        {
            context.warning(
                "password_hash(): The \"salt\" option has been ignored, since providing a custom \
                 salt is no longer supported"
                    .to_string(),
            );
        }
        let hash = off_thread(move || hash_password(algorithm, &options, &password)).await?;
        Ok(Value::from(hash.map_err(NativeError::value_error)?))
    })
}

/// A new hash with a random salt; an error message if the library refuses.
fn hash_password(
    algorithm: Algorithm,
    options: &Options,
    password: &[u8],
) -> Result<String, String> {
    let mut salt = [0; SALT_BYTES];
    getrandom::fill(&mut salt).map_err(|error| error.to_string())?;
    let argon2 = match algorithm {
        Algorithm::Bcrypt => {
            return bcrypt::hash_with_salt(password, options.cost as u32, salt)
                .map(|parts| parts.format_for_version(bcrypt::Version::TwoY))
                .map_err(|error| error.to_string());
        }
        Algorithm::Argon2i => argon2::Algorithm::Argon2i,
        Algorithm::Argon2id => argon2::Algorithm::Argon2id,
    };
    // `validate` has kept all three within u32.
    let params = Params::new(
        options.memory as u32,
        options.time as u32,
        options.threads as u32,
        Some(ARGON2_OUTPUT_BYTES),
    )
    .map_err(|error| error.to_string())?;
    let salt = SaltString::encode_b64(&salt).map_err(|error| error.to_string())?;
    Argon2::new(argon2, Version::V0x13, params)
        .hash_password(password, &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| error.to_string())
}

/// `password_verify(string $password, string $hash): bool`
const PASSWORD_VERIFY: FunctionEntry = FunctionEntry::asynchronous(
    "password_verify",
    &[
        Param::new("password", ParamType::String),
        Param::new("hash", ParamType::String),
    ],
    password_verify,
);

fn password_verify(_: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let password = args::bytes(&arguments, 0).to_vec();
        let hash = args::bytes(&arguments, 1).to_vec();
        let verified = off_thread(move || verify(&password, &hash)).await?;
        Ok(Value::Bool(verified))
    })
}

fn verify(password: &[u8], hash: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(hash) else {
        return false;
    };
    match identify(hash) {
        Some(Algorithm::Argon2i | Algorithm::Argon2id) => PasswordHash::new(text)
            .is_ok_and(|parsed| Argon2::default().verify_password(password, &parsed).is_ok()),
        // Bcrypt, and every other scheme crypt() knows.
        _ if hash.len() < SHORTEST_CRYPT => false,
        _ => crypt_bytes(password, hash).as_bytes().ct_eq(hash).into(),
    }
}

/// `crypt(string $string, string $salt): string`
const CRYPT: FunctionEntry = FunctionEntry::asynchronous(
    "crypt",
    &[
        Param::new("string", ParamType::String),
        Param::new("salt", ParamType::String),
    ],
    crypt,
);

fn crypt(_: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let password = args::bytes(&arguments, 0).to_vec();
        let salt = args::bytes(&arguments, 1).to_vec();
        let hash = off_thread(move || crypt_bytes(&password, &salt)).await?;
        Ok(Value::from(hash))
    })
}

/// What `crypt()` returns. C strings end at a NUL byte, so both inputs do
/// too. On failure PHP returns `*0`, or `*1` if the salt was `*0`, so that
/// a failure can never equal the salt.
fn crypt_bytes(password: &[u8], salt: &[u8]) -> String {
    let (password, salt) = (until_nul(password), until_nul(salt));
    let failure = if salt.starts_with(b"*0") { "*1" } else { "*0" };
    let Ok(salt) = std::str::from_utf8(salt) else {
        return failure.to_string();
    };
    // A salt that names a scheme must name one PHP has.
    const SCHEMES: [&str; 6] = ["$1$", "$2a$", "$2b$", "$2y$", "$5$", "$6$"];
    if salt.starts_with('$') && !SCHEMES.iter().any(|scheme| salt.starts_with(scheme)) {
        return failure.to_string();
    }
    pwhash::unix::crypt(password, salt).unwrap_or_else(|_| failure.to_string())
}

/// The bytes before the first NUL, as C sees a string.
fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes.split(|&byte| byte == 0).next().unwrap_or_default()
}
