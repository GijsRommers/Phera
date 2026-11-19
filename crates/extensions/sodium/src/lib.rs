//! PHP's Sodium extension, for example `sodium_bin2hex("\\x00\\xff")` returns `'00ff'`.

use phera_extension_api::{ClassEntry, Extension, FunctionEntry, NativeError, Registry};

mod encoding;

pub struct Sodium;

impl Extension for Sodium {
    fn name(&self) -> &'static str {
        "sodium"
    }

    fn register(&self, registry: &mut Registry) {
        registry.class(ClassEntry {
            parent: Some("Exception"),
            ..ClassEntry::class("SodiumException")
        });
        register(registry, encoding::FUNCTIONS);
        for &(name, value) in encoding::CONSTANTS {
            registry.constant(name, value);
        }
    }
}

fn register(registry: &mut Registry, functions: &[(FunctionEntry, &'static str)]) {
    for &(entry, return_type) in functions {
        registry.function(entry);
        registry.describe_function(entry.name, return_type, None);
    }
}

fn exception(message: impl Into<String>) -> NativeError {
    NativeError::throw("SodiumException", message)
}

fn argument(function: &str, position: usize, name: &str, reason: &str) -> NativeError {
    exception(format!(
        "{function}(): Argument #{position} (${name}) {reason}"
    ))
}
