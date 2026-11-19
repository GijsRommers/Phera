//! PHP session configuration and identifiers, for example `session_name('APPSESSID')`.
//!
//! Mutable settings, identifiers, handlers, and open files belong to a single request.

mod classes;
mod codec;
mod config_functions;
mod cookie_params;
mod file_handler;
mod files;
mod headers;
mod id;
mod lifecycle;
mod quantity;
mod regenerate;
mod settings;
mod start;
mod state;
mod storage;
mod user_handler;
mod variables;

#[cfg(test)]
mod tests;

use phera_extension_api::{Extension, FunctionEntry, Registry, Value};

pub struct Session;

impl Extension for Session {
    fn name(&self) -> &'static str {
        "session"
    }

    fn register(&self, registry: &mut Registry) {
        registry.ini_family(b"session", settings::ini);
        registry.constant("PHP_SESSION_DISABLED", 0);
        registry.constant("PHP_SESSION_NONE", 1);
        registry.constant("PHP_SESSION_ACTIVE", 2);
        registry.function(FunctionEntry::sync("session_status", &[], |context, _| {
            Ok(Value::Int(if state::active(context) { 2 } else { 1 }))
        }));
        for group in [
            id::FUNCTIONS,
            config_functions::FUNCTIONS,
            lifecycle::FUNCTIONS,
            cookie_params::FUNCTIONS,
            variables::FUNCTIONS,
            user_handler::FUNCTIONS,
        ] {
            for &function in group {
                registry.function(function);
            }
        }
        registry.function(start::FUNCTION);
        registry.function(regenerate::FUNCTION);
        for (function, return_type) in RETURNS {
            registry.describe_function(function, return_type, None);
        }
        for &class in classes::CLASSES {
            registry.class(class);
        }
    }
}

const RETURNS: &[(&str, &str)] = &[
    ("session_name", "string|false"),
    ("session_module_name", "string|false"),
    ("session_save_path", "string|false"),
    ("session_id", "string|false"),
    ("session_create_id", "string|false"),
    ("session_regenerate_id", "bool"),
    ("session_encode", "string|false"),
    ("session_decode", "bool"),
    ("session_destroy", "bool"),
    ("session_unset", "bool"),
    ("session_gc", "int|false"),
    ("session_get_cookie_params", "array"),
    ("session_write_close", "bool"),
    ("session_abort", "bool"),
    ("session_reset", "bool"),
    ("session_status", "int"),
    ("session_register_shutdown", "void"),
    ("session_commit", "bool"),
    ("session_set_save_handler", "bool"),
    ("session_cache_limiter", "string|false"),
    ("session_cache_expire", "int|false"),
    ("session_set_cookie_params", "bool"),
    ("session_start", "bool"),
];
