//! PHP's random extension: the procedural functions, `Random\Randomizer`
//! and its four engines. Seeded generators give PHP's exact sequences:
//!
//! ```php
//! mt_srand(42);
//! echo mt_rand(1, 100);                       // what PHP prints
//! $r = new Random\Randomizer(new Random\Engine\Xoshiro256StarStar(42));
//! echo $r->getInt(1, 100);                    // what PHP prints
//! echo bin2hex(random_bytes(8));              // from the operating system
//! ```

mod collections;
mod engine_classes;
mod engines;
mod floats;
mod mt19937;
mod procedural;
mod randomizer;
mod serialization;

use phera_extension_api::{ClassEntry, Extension, Registry, Value};

pub struct Random;

impl Extension for Random {
    fn name(&self) -> &'static str {
        "random"
    }

    fn register(&self, registry: &mut Registry) {
        registry.constant("MT_RAND_MT19937", mt19937::MT_RAND_MT19937);
        // Error and Exception come from the core extension.
        for &class in ERRORS.iter().chain(engine_classes::CLASSES) {
            registry.class(class);
        }
        registry.class(floats::INTERVAL_BOUNDARY);
        registry.class(randomizer::RANDOMIZER);
        for functions in [procedural::FUNCTIONS, collections::FUNCTIONS] {
            for &entry in functions {
                registry.function(entry);
            }
        }
    }
}

const ERRORS: &[ClassEntry] = &[
    ClassEntry {
        parent: Some("Error"),
        ..ClassEntry::class("Random\\RandomError")
    },
    // An engine that keeps returning numbers a range cannot use.
    ClassEntry {
        parent: Some("Random\\RandomError"),
        ..ClassEntry::class("Random\\BrokenRandomEngineError")
    },
    // The operating system could not provide random bytes.
    ClassEntry {
        parent: Some("Exception"),
        ..ClassEntry::class("Random\\RandomException")
    },
];

/// How PHP names a wrong argument in a `TypeError`: `true` and `false`
/// rather than `bool`, otherwise the type or the class.
fn given(value: &Value) -> String {
    match value {
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        other => other.type_name(),
    }
}
