//! PHP session configuration and identifiers, for example `session_name('APPSESSID')`.
//!
//! Mutable settings, identifiers, handlers, and open files belong to a single request.

mod classes;
mod codec;
mod cookie_params;
mod file_handler;
mod files;
mod headers;
mod id;
mod lifecycle;
mod regenerate;
mod settings;
mod start;
mod state;
mod storage;
mod user_handler;
mod variables;

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
            settings::FUNCTIONS,
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
        for &class in classes::CLASSES {
            registry.class(class);
        }
    }
}
