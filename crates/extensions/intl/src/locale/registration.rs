//! PHP method names and procedural aliases for locale queries.
//!
//! ```php
//! locale_get_region('en_GB') === Locale::getRegion('en_GB'); // true
//! ```

use phera_extension_api::Registry;
use phera_extension_api::{
    ClassEntry, ConstantValue, FunctionEntry, MethodEntry, Param, ParamType,
};

use super::{Query, compose, get_default, locale_display, locale_matching, query, set_default};
use locale_display::Display;

const LOCALE_VALUE: Param = Param::new("locale", ParamType::String);
const LOCALE: &[Param] = &[LOCALE_VALUE];
const COMPOSE: &[Param] = &[Param::new("subtags", ParamType::Array)];
const DISPLAY: &[Param] = &[
    LOCALE_VALUE,
    Param::new("displayLocale", ParamType::String)
        .nullable()
        .with_default(ConstantValue::Null),
];
const CANONICALIZE: Param =
    Param::new("canonicalize", ParamType::Bool).with_default(ConstantValue::Bool(false));
const FILTER: &[Param] = &[
    Param::new("languageTag", ParamType::String),
    LOCALE_VALUE,
    CANONICALIZE,
];
const LOOKUP: &[Param] = &[
    Param::new("languageTag", ParamType::Array),
    LOCALE_VALUE,
    CANONICALIZE,
    Param::new("defaultLocale", ParamType::String)
        .nullable()
        .with_default(ConstantValue::Null),
];
const HEADER: &[Param] = &[Param::new("header", ParamType::String)];

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        constants: &[
            ("ACTUAL_LOCALE", ConstantValue::Int(0)),
            ("VALID_LOCALE", ConstantValue::Int(1)),
            ("DEFAULT_LOCALE", ConstantValue::Null),
            ("LANG_TAG", ConstantValue::String("language")),
            ("EXTLANG_TAG", ConstantValue::String("extlang")),
            ("SCRIPT_TAG", ConstantValue::String("script")),
            ("REGION_TAG", ConstantValue::String("region")),
            ("VARIANT_TAG", ConstantValue::String("variant")),
            (
                "GRANDFATHERED_LANG_TAG",
                ConstantValue::String("grandfathered"),
            ),
            ("PRIVATE_TAG", ConstantValue::String("private")),
        ],
        methods: const {
            &[
                MethodEntry::public("getDefault", &[], |c, _, a| {
                    get_default(c, a, "Locale::getDefault")
                })
                .static_()
                .tentative_return("string"),
                MethodEntry::public("setDefault", LOCALE, |c, _, a| {
                    set_default(c, a, "Locale::setDefault")
                })
                .static_()
                .tentative_return("true"),
                MethodEntry::public("getPrimaryLanguage", LOCALE, |c, _, a| {
                    query(c, a, "Locale::getPrimaryLanguage", Query::Language)
                })
                .static_()
                .tentative_return("?string"),
                MethodEntry::public("getScript", LOCALE, |c, _, a| {
                    query(c, a, "Locale::getScript", Query::Script)
                })
                .static_()
                .tentative_return("?string"),
                MethodEntry::public("getRegion", LOCALE, |c, _, a| {
                    query(c, a, "Locale::getRegion", Query::Region)
                })
                .static_()
                .tentative_return("?string"),
                MethodEntry::public("getAllVariants", LOCALE, |c, _, a| {
                    query(c, a, "Locale::getAllVariants", Query::Variants)
                })
                .static_()
                .tentative_return("?array"),
                MethodEntry::public("parseLocale", LOCALE, |c, _, a| {
                    query(c, a, "Locale::parseLocale", Query::Parse)
                })
                .static_()
                .tentative_return("?array"),
                MethodEntry::public("canonicalize", LOCALE, |c, _, a| {
                    query(c, a, "Locale::canonicalize", Query::Canonicalize)
                })
                .static_()
                .tentative_return("?string"),
                MethodEntry::public("getKeywords", LOCALE, |c, _, a| {
                    query(c, a, "Locale::getKeywords", Query::Keywords)
                })
                .static_()
                .tentative_return("array|false|null"),
                MethodEntry::public("isRightToLeft", LOCALE, |c, _, a| {
                    query(c, a, "Locale::isRightToLeft", Query::RightToLeft)
                })
                .static_()
                .returning("bool"),
                MethodEntry::public("addLikelySubtags", LOCALE, |c, _, a| {
                    query(c, a, "Locale::addLikelySubtags", Query::LikelySubtags)
                })
                .static_()
                .returning("string|false"),
                MethodEntry::public("minimizeSubtags", LOCALE, |c, _, a| {
                    query(c, a, "Locale::minimizeSubtags", Query::Minimize)
                })
                .static_()
                .returning("string|false"),
                MethodEntry::public("composeLocale", COMPOSE, |c, _, a| {
                    compose(c, a, "Locale::composeLocale")
                })
                .static_()
                .tentative_return("string|false"),
                MethodEntry::public("getDisplayName", DISPLAY, |c, _, a| {
                    locale_display::display(c, a, "Locale::getDisplayName", Display::Name)
                })
                .static_()
                .tentative_return("string|false"),
                MethodEntry::public("getDisplayLanguage", DISPLAY, |c, _, a| {
                    locale_display::display(c, a, "Locale::getDisplayLanguage", Display::Language)
                })
                .static_()
                .tentative_return("string|false"),
                MethodEntry::public("getDisplayScript", DISPLAY, |c, _, a| {
                    locale_display::display(c, a, "Locale::getDisplayScript", Display::Script)
                })
                .static_()
                .tentative_return("string|false"),
                MethodEntry::public("getDisplayRegion", DISPLAY, |c, _, a| {
                    locale_display::display(c, a, "Locale::getDisplayRegion", Display::Region)
                })
                .static_()
                .tentative_return("string|false"),
                MethodEntry::public("getDisplayVariant", DISPLAY, |c, _, a| {
                    locale_display::display(c, a, "Locale::getDisplayVariant", Display::Variant)
                })
                .static_()
                .tentative_return("string|false"),
                MethodEntry::public("filterMatches", FILTER, |c, _, a| {
                    locale_matching::filter(c, a, "Locale::filterMatches")
                })
                .static_()
                .tentative_return("?bool"),
                MethodEntry::public("lookup", LOOKUP, |c, _, a| {
                    locale_matching::lookup(c, a, "Locale::lookup")
                })
                .static_()
                .tentative_return("?string"),
                MethodEntry::public("acceptFromHttp", HEADER, |c, _, a| {
                    locale_matching::accept(c, a, "Locale::acceptFromHttp")
                })
                .static_()
                .tentative_return("string|false"),
            ]
        },
        ..ClassEntry::class("Locale")
    });
    for (entry, result) in [
        (
            FunctionEntry::sync("locale_get_default", &[], |c, a| {
                get_default(c, a, "locale_get_default")
            }),
            "string",
        ),
        (
            FunctionEntry::sync("locale_set_default", LOCALE, |c, a| {
                set_default(c, a, "locale_set_default")
            }),
            "true",
        ),
        (
            FunctionEntry::sync("locale_get_primary_language", LOCALE, |c, a| {
                query(c, a, "locale_get_primary_language", Query::Language)
            }),
            "?string",
        ),
        (
            FunctionEntry::sync("locale_get_script", LOCALE, |c, a| {
                query(c, a, "locale_get_script", Query::Script)
            }),
            "?string",
        ),
        (
            FunctionEntry::sync("locale_get_region", LOCALE, |c, a| {
                query(c, a, "locale_get_region", Query::Region)
            }),
            "?string",
        ),
        (
            FunctionEntry::sync("locale_get_all_variants", LOCALE, |c, a| {
                query(c, a, "locale_get_all_variants", Query::Variants)
            }),
            "?array",
        ),
        (
            FunctionEntry::sync("locale_parse", LOCALE, |c, a| {
                query(c, a, "locale_parse", Query::Parse)
            }),
            "?array",
        ),
        (
            FunctionEntry::sync("locale_canonicalize", LOCALE, |c, a| {
                query(c, a, "locale_canonicalize", Query::Canonicalize)
            }),
            "?string",
        ),
        (
            FunctionEntry::sync("locale_get_keywords", LOCALE, |c, a| {
                query(c, a, "locale_get_keywords", Query::Keywords)
            }),
            "array|false|null",
        ),
        (
            FunctionEntry::sync("locale_is_right_to_left", LOCALE, |c, a| {
                query(c, a, "locale_is_right_to_left", Query::RightToLeft)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("locale_add_likely_subtags", LOCALE, |c, a| {
                query(c, a, "locale_add_likely_subtags", Query::LikelySubtags)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("locale_minimize_subtags", LOCALE, |c, a| {
                query(c, a, "locale_minimize_subtags", Query::Minimize)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("locale_compose", COMPOSE, |c, a| {
                compose(c, a, "locale_compose")
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("locale_get_display_name", DISPLAY, |c, a| {
                locale_display::display(c, a, "locale_get_display_name", Display::Name)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("locale_get_display_language", DISPLAY, |c, a| {
                locale_display::display(c, a, "locale_get_display_language", Display::Language)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("locale_get_display_script", DISPLAY, |c, a| {
                locale_display::display(c, a, "locale_get_display_script", Display::Script)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("locale_get_display_region", DISPLAY, |c, a| {
                locale_display::display(c, a, "locale_get_display_region", Display::Region)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("locale_get_display_variant", DISPLAY, |c, a| {
                locale_display::display(c, a, "locale_get_display_variant", Display::Variant)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("locale_filter_matches", FILTER, |c, a| {
                locale_matching::filter(c, a, "locale_filter_matches")
            }),
            "?bool",
        ),
        (
            FunctionEntry::sync("locale_lookup", LOOKUP, |c, a| {
                locale_matching::lookup(c, a, "locale_lookup")
            }),
            "?string",
        ),
        (
            FunctionEntry::sync("locale_accept_from_http", HEADER, |c, a| {
                locale_matching::accept(c, a, "locale_accept_from_http")
            }),
            "string|false",
        ),
    ] {
        registry.function(entry);
        registry.describe_function(entry.name, result, None);
    }
}
