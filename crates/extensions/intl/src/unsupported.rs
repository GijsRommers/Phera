//! Explicit errors for intl classes outside the supported scope.
//!
//! ```php
//! new UConverter(); // Phera\NotSupportedException names UConverter
//! ```

use phera_extension_api::{ClassEntry, MethodEntry, NativeError, Param, ParamType, Registry};

const CONVERTER_ENCODINGS: &[Param] = &[
    Param::new("destination_encoding", ParamType::String)
        .nullable()
        .optional(),
    Param::new("source_encoding", ParamType::String)
        .nullable()
        .optional(),
];
const TRANSCODE: &[Param] = &[
    Param::new("str", ParamType::String),
    Param::new("toEncoding", ParamType::String),
    Param::new("fromEncoding", ParamType::String),
    Param::new("options", ParamType::Array)
        .nullable()
        .optional(),
];
const PATTERN_LOCALE: &[Param] = &[Param::new("locale", ParamType::String)
    .nullable()
    .optional()];

const ALIAS_NAME: &[Param] = &[Param::new("name", ParamType::String)];

const REASON: &[Param] = &[Param::new("reason", ParamType::Int)];

const SKELETON: &[Param] = &[Param::new("skeleton", ParamType::String)];

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
    registry.class(ClassEntry {
        methods: const {
            &[
                MethodEntry::public("__construct", CONVERTER_ENCODINGS, |_, _, _| {
                    Err(item("UConverter"))
                }),
                MethodEntry::public("getAliases", ALIAS_NAME, |_, _, _| {
                    Err(item("UConverter::getAliases"))
                })
                .static_(),
                MethodEntry::public("getAvailable", &[], |_, _, _| {
                    Err(item("UConverter::getAvailable"))
                })
                .static_(),
                MethodEntry::public("getStandards", &[], |_, _, _| {
                    Err(item("UConverter::getStandards"))
                })
                .static_(),
                MethodEntry::public("reasonText", REASON, |_, _, _| {
                    Err(item("UConverter::reasonText"))
                })
                .static_(),
                MethodEntry::public("transcode", TRANSCODE, |_, _, _| {
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
                MethodEntry::public("__construct", PATTERN_LOCALE, |_, _, _| {
                    Err(item("IntlDatePatternGenerator"))
                }),
                MethodEntry::public("create", PATTERN_LOCALE, |_, _, _| {
                    Err(item("IntlDatePatternGenerator::create"))
                })
                .static_(),
                MethodEntry::public("getBestPattern", SKELETON, |_, _, _| {
                    Err(item("IntlDatePatternGenerator::getBestPattern"))
                }),
            ]
        },
        ..ClassEntry::class("IntlDatePatternGenerator")
    });
}
