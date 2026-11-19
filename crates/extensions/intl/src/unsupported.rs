//! Explicit errors for intl classes outside the supported scope.
//!
//! ```php
//! new UConverter(); // Phera\NotSupportedException names UConverter
//! ```

use phera_extension_api::{ClassEntry, MethodEntry, NativeError, Param, ParamType, Registry};

pub fn item(name: &str) -> NativeError {
    NativeError::throw(
        "Phera\\NotSupportedException",
        format!("{name} is not supported by Phera"),
    )
}

pub fn register(registry: &mut Registry) {
    const ARGUMENTS: &[Param] = &[Param::new("arguments", ParamType::Mixed).variadic()];
    registry.class(ClassEntry {
        methods: const {
            &[
                MethodEntry::public("__construct", ARGUMENTS, |_, _, _| Err(item("UConverter"))),
                MethodEntry::public("getAliases", ARGUMENTS, |_, _, _| {
                    Err(item("UConverter::getAliases"))
                })
                .static_(),
                MethodEntry::public("getAvailable", ARGUMENTS, |_, _, _| {
                    Err(item("UConverter::getAvailable"))
                })
                .static_(),
                MethodEntry::public("getStandards", ARGUMENTS, |_, _, _| {
                    Err(item("UConverter::getStandards"))
                })
                .static_(),
                MethodEntry::public("reasonText", ARGUMENTS, |_, _, _| {
                    Err(item("UConverter::reasonText"))
                })
                .static_(),
                MethodEntry::public("transcode", ARGUMENTS, |_, _, _| {
                    Err(item("UConverter::transcode"))
                })
                .static_(),
            ]
        },
        ..ClassEntry::class("UConverter")
    });
    registry.class(ClassEntry {
        methods: const {
            &[
                MethodEntry::public("__construct", ARGUMENTS, |_, _, _| {
                    Err(item("IntlDatePatternGenerator"))
                }),
                MethodEntry::public("create", ARGUMENTS, |_, _, _| {
                    Err(item("IntlDatePatternGenerator::create"))
                })
                .static_(),
                MethodEntry::public("getBestPattern", ARGUMENTS, |_, _, _| {
                    Err(item("IntlDatePatternGenerator::getBestPattern"))
                }),
            ]
        },
        ..ClassEntry::class("IntlDatePatternGenerator")
    });
    registry.class(ClassEntry {
        methods: const {
            &[MethodEntry::public("__construct", ARGUMENTS, |_, _, _| {
                Err(item("IntlPartsIterator"))
            })]
        },
        ..ClassEntry::class("IntlPartsIterator")
    });
}
