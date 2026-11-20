//! Explicit errors for intl classes outside the supported scope.
//!
//! ```php
//! new Spoofchecker(); // Phera\NotSupportedException names Spoofchecker
//! ```

use phera_extension_api::{ClassEntry, MethodEntry, NativeError, Registry};

pub fn item(name: &str) -> NativeError {
    NativeError::throw(
        "Phera\\NotSupportedException",
        format!("{name} is not supported by Phera"),
    )
}

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        methods: const {
            &[MethodEntry::public("__construct", &[], |_, _, _| {
                Err(item("Spoofchecker"))
            })]
        },
        ..ClassEntry::class("Spoofchecker")
    });
}
