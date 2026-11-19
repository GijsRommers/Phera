//! Date methods with tentative late-static return types in PHP 8.6.
//!
//! ```php
//! $method = new ReflectionMethod(DateTimeImmutable::class, 'modify');
//! echo $method->getTentativeReturnType(); // static
//! ```

use phera_extension_api::{Availability, Change, Registry};

const IMMUTABLE: &[&str] = &[
    "__set_state",
    "modify",
    "add",
    "sub",
    "setTimezone",
    "setTime",
    "setDate",
    "setISODate",
    "setTimestamp",
    "createFromInterface",
];

pub(super) fn register(registry: &mut Registry) {
    if !registry.level().has(Change::DateTentativeStaticReturnTypes) {
        return;
    }
    for (class, methods) in [
        ("DateTime", &["createFromInterface"][..]),
        ("DateTimeImmutable", IMMUTABLE),
        ("DateTimeZone", &["__set_state"]),
        ("DateInterval", &["__set_state"]),
        ("DatePeriod", &["__set_state"]),
    ] {
        for name in methods {
            let entry = registry
                .get_class(class)
                .and_then(|entry| entry.methods.iter().find(|method| method.name == *name))
                .copied();
            if let Some(method) = entry {
                registry.method_at(
                    Availability::Since86,
                    class,
                    method.tentative_return("static"),
                );
            }
        }
    }
}
