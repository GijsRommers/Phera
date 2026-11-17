//! Password constants retain PHP's exact values: `PASSWORD_DEFAULT === '2y'`.
use crate::{password_info, passwords};
use phera_extension_api::{FunctionEntry, Param, ParamType, Registry};
pub fn register(registry: &mut Registry) {
    for (name, value) in [
        ("PASSWORD_DEFAULT", "2y"),
        ("PASSWORD_BCRYPT", "2y"),
        ("PASSWORD_ARGON2I", "argon2i"),
        ("PASSWORD_ARGON2ID", "argon2id"),
        ("PASSWORD_ARGON2_PROVIDER", "standard"),
    ] {
        registry.constant(name, value);
    }
    for (name, value) in [
        ("PASSWORD_BCRYPT_DEFAULT_COST", 12i64),
        ("PASSWORD_ARGON2_DEFAULT_MEMORY_COST", 65536),
        ("PASSWORD_ARGON2_DEFAULT_TIME_COST", 4),
        ("PASSWORD_ARGON2_DEFAULT_THREADS", 1),
    ] {
        registry.constant(name, value);
    }
    for entry in FUNCTIONS {
        registry.function(*entry);
    }
}
const HASH: &[Param] = &[
    Param::new("password", ParamType::String),
    Param::new("algo", ParamType::IntOrString).nullable(),
    Param::new("options", ParamType::Array).optional(),
];
const REHASH: &[Param] = &[
    Param::new("hash", ParamType::String),
    Param::new("algo", ParamType::IntOrString).nullable(),
    Param::new("options", ParamType::Array).optional(),
];
const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::asynchronous("password_hash", HASH, passwords::hash),
    FunctionEntry::asynchronous(
        "password_verify",
        &[
            Param::new("password", ParamType::String),
            Param::new("hash", ParamType::String),
        ],
        passwords::verify,
    ),
    FunctionEntry::asynchronous(
        "crypt",
        &[
            Param::new("string", ParamType::String),
            Param::new("salt", ParamType::String),
        ],
        passwords::crypt,
    ),
    FunctionEntry::sync(
        "password_get_info",
        &[Param::new("hash", ParamType::String)],
        password_info::info,
    ),
    FunctionEntry::sync("password_needs_rehash", REHASH, password_info::needs_rehash),
    FunctionEntry::sync("password_algos", &[], password_info::algos),
];
