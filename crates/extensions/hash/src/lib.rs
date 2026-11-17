//! PHP hashing: `hash('sha256', 'hello')` returns a lowercase hexadecimal digest.
mod algorithms;
mod files;
mod functions;
mod hmac;
mod incremental;
mod options;
mod password_info;
mod password_options;
mod password_registry;
mod passwords;
use phera_extension_api::{
    ClassEntry, ClassKind, Extension, FunctionEntry, MethodEntry, Param, ParamType, Registry,
    Visibility,
};
pub struct Hash;
impl Extension for Hash {
    fn name(&self) -> &'static str {
        "hash"
    }
    fn register(&self, registry: &mut Registry) {
        registry.constant("HASH_HMAC", 1i64);
        password_registry::register(registry);
        registry.class(CLASS);
        for entry in FUNCTIONS {
            registry.function(*entry);
        }
    }
}
const CLASS: ClassEntry = ClassEntry {
    kind: ClassKind::FinalClass,
    methods: &[
        MethodEntry::public("__construct", &[], incremental::construct)
            .with_visibility(Visibility::Private),
    ],
    ..ClassEntry::class("HashContext")
};

const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::sync(
        "hash",
        &[
            Param::new("algo", ParamType::String),
            Param::new("data", ParamType::String),
            Param::new("binary", ParamType::Bool).optional(),
            Param::new("options", ParamType::Array).optional(),
        ],
        functions::hash,
    ),
    FunctionEntry::asynchronous(
        "hash_file",
        &[
            Param::new("algo", ParamType::String),
            Param::new("filename", ParamType::String),
            Param::new("binary", ParamType::Bool).optional(),
            Param::new("options", ParamType::Array).optional(),
        ],
        files::hash_file,
    ),
    FunctionEntry::sync(
        "hash_hmac",
        &[
            Param::new("algo", ParamType::String),
            Param::new("data", ParamType::String),
            Param::new("key", ParamType::String),
            Param::new("binary", ParamType::Bool).optional(),
        ],
        functions::hash_hmac,
    ),
    FunctionEntry::asynchronous(
        "hash_hmac_file",
        &[
            Param::new("algo", ParamType::String),
            Param::new("filename", ParamType::String),
            Param::new("key", ParamType::String),
            Param::new("binary", ParamType::Bool).optional(),
        ],
        files::hmac_file,
    ),
    FunctionEntry::sync("hash_algos", &[], functions::algos),
    FunctionEntry::sync("hash_hmac_algos", &[], functions::hmac_algos),
    FunctionEntry::sync(
        "hash_equals",
        &[
            Param::new("known_string", ParamType::Mixed),
            Param::new("user_string", ParamType::Mixed),
        ],
        functions::equals,
    ),
    FunctionEntry::sync(
        "md5",
        &[
            Param::new("string", ParamType::String),
            Param::new("binary", ParamType::Bool).optional(),
        ],
        functions::md5,
    ),
    FunctionEntry::sync(
        "sha1",
        &[
            Param::new("string", ParamType::String),
            Param::new("binary", ParamType::Bool).optional(),
        ],
        functions::sha1,
    ),
    FunctionEntry::asynchronous(
        "md5_file",
        &[
            Param::new("filename", ParamType::String),
            Param::new("binary", ParamType::Bool).optional(),
        ],
        files::md5_file,
    ),
    FunctionEntry::asynchronous(
        "sha1_file",
        &[
            Param::new("filename", ParamType::String),
            Param::new("binary", ParamType::Bool).optional(),
        ],
        files::sha1_file,
    ),
    FunctionEntry::sync(
        "hash_pbkdf2",
        &[
            Param::new("algo", ParamType::String),
            Param::new("password", ParamType::String),
            Param::new("salt", ParamType::String),
            Param::new("iterations", ParamType::Int),
            Param::new("length", ParamType::Int).optional(),
            Param::new("binary", ParamType::Bool).optional(),
            Param::new("options", ParamType::Array).optional(),
        ],
        functions::pbkdf2,
    ),
    FunctionEntry::sync(
        "hash_hkdf",
        &[
            Param::new("algo", ParamType::String),
            Param::new("key", ParamType::String),
            Param::new("length", ParamType::Int).optional(),
            Param::new("info", ParamType::String).optional(),
            Param::new("salt", ParamType::String).optional(),
        ],
        functions::hkdf,
    ),
    FunctionEntry::sync(
        "hash_init",
        &[
            Param::new("algo", ParamType::String),
            Param::new("flags", ParamType::Int).optional(),
            Param::new("key", ParamType::String).optional(),
            Param::new("options", ParamType::Array).optional(),
        ],
        incremental::init,
    ),
    FunctionEntry::sync(
        "hash_update",
        &[
            Param::new("context", ParamType::Mixed),
            Param::new("data", ParamType::String),
        ],
        incremental::update,
    ),
    FunctionEntry::sync(
        "hash_final",
        &[
            Param::new("context", ParamType::Mixed),
            Param::new("binary", ParamType::Bool).optional(),
        ],
        incremental::finish,
    ),
    FunctionEntry::sync(
        "hash_copy",
        &[Param::new("context", ParamType::Mixed)],
        incremental::copy,
    ),
    FunctionEntry::asynchronous(
        "hash_update_file",
        &[
            Param::new("context", ParamType::Mixed),
            Param::new("filename", ParamType::String),
            Param::new("stream_context", ParamType::Mixed).optional(),
        ],
        files::update_file,
    ),
    FunctionEntry::sync(
        "hash_update_stream",
        &[
            Param::new("context", ParamType::Mixed),
            Param::new("stream", ParamType::Mixed),
            Param::new("length", ParamType::Int).optional(),
        ],
        incremental::stream,
    ),
];
