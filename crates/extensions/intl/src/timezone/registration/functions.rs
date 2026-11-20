//! PHP's procedural time-zone aliases share the class handlers.

use phera_extension_api::{FunctionEntry, Registry};

use super::*;

pub(super) fn register(registry: &mut Registry) {
    for (entry, return_type) in [
        (
            FunctionEntry::sync("intltz_create_default", &[], |cx, values| {
                factory(cx, values, Factory::Default)
            }),
            "IntlTimeZone",
        ),
        (
            FunctionEntry::sync("intltz_create_time_zone", ID_ONLY, |cx, values| {
                factory(cx, values, Factory::Named)
            }),
            "?IntlTimeZone",
        ),
        (
            FunctionEntry::sync("intltz_get_gmt", &[], |cx, values| {
                factory(cx, values, Factory::Gmt)
            }),
            "IntlTimeZone",
        ),
        (
            FunctionEntry::sync("intltz_get_unknown", &[], |cx, values| {
                factory(cx, values, Factory::Unknown)
            }),
            "IntlTimeZone",
        ),
        (
            FunctionEntry::sync("intltz_from_date_time_zone", DATE_ZONE, |cx, values| {
                factory(cx, values, Factory::FromDate)
            }),
            "?IntlTimeZone",
        ),
        (
            FunctionEntry::sync("intltz_count_equivalent_ids", ID_ONLY, |cx, values| {
                ids::invoke(cx, values, "intltz_count_equivalent_ids", ids::Op::Count)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("intltz_get_equivalent_id", EQUIVALENT, |cx, values| {
                ids::invoke(cx, values, "intltz_get_equivalent_id", ids::Op::Equivalent)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("intltz_get_canonical_id", CANONICAL, |cx, values| {
                ids::invoke(cx, values, "intltz_get_canonical_id", ids::Op::Canonical)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("intltz_get_iana_id", ID_ONLY, |cx, values| {
                ids::invoke(cx, values, "intltz_get_iana_id", ids::Op::Iana)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("intltz_get_region", ID_ONLY, |cx, values| {
                ids::invoke(cx, values, "intltz_get_region", ids::Op::Region)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("intltz_get_tz_data_version", &[], |cx, values| {
                ids::invoke(cx, values, "intltz_get_tz_data_version", ids::Op::Version)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("intltz_get_windows_id", ID_ONLY, |cx, values| {
                ids::invoke(cx, values, "intltz_get_windows_id", ids::Op::Windows)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("intltz_get_id_for_windows_id", WINDOWS, |cx, values| {
                ids::invoke(
                    cx,
                    values,
                    "intltz_get_id_for_windows_id",
                    ids::Op::FromWindows,
                )
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("intltz_get_id", INSTANCE, |cx, values| {
                dispatch(cx, None, values, "intltz_get_id", Op::Id)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("intltz_get_raw_offset", INSTANCE, |cx, values| {
                dispatch(cx, None, values, "intltz_get_raw_offset", Op::Raw)
            }),
            "int",
        ),
        (
            FunctionEntry::sync("intltz_get_dst_savings", INSTANCE, |cx, values| {
                dispatch(cx, None, values, "intltz_get_dst_savings", Op::Savings)
            }),
            "int",
        ),
        (
            FunctionEntry::sync("intltz_use_daylight_time", INSTANCE, |cx, values| {
                dispatch(cx, None, values, "intltz_use_daylight_time", Op::Daylight)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("intltz_get_offset", FN_OFFSET, |cx, values| {
                dispatch(cx, None, values, "intltz_get_offset", Op::Offset)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("intltz_get_display_name", FN_DISPLAY, |cx, values| {
                dispatch(cx, None, values, "intltz_get_display_name", Op::Display)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("intltz_has_same_rules", FN_RULES, |cx, values| {
                dispatch(cx, None, values, "intltz_has_same_rules", Op::SameRules)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("intltz_to_date_time_zone", INSTANCE, |cx, values| {
                dispatch(cx, None, values, "intltz_to_date_time_zone", Op::ToDate)
            }),
            "DateTimeZone|false",
        ),
        (
            FunctionEntry::sync("intltz_get_error_code", INSTANCE, |cx, values| {
                dispatch(cx, None, values, "intltz_get_error_code", Op::ErrorCode)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("intltz_get_error_message", INSTANCE, |cx, values| {
                dispatch(
                    cx,
                    None,
                    values,
                    "intltz_get_error_message",
                    Op::ErrorMessage,
                )
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("intltz_create_enumeration", ENUMERATE, |cx, values| {
                ids::enumerate(cx, values, "intltz_create_enumeration", false)
            }),
            "IntlIterator|false",
        ),
        (
            FunctionEntry::sync(
                "intltz_create_time_zone_id_enumeration",
                ENUMERATE_TYPED,
                |cx, values| {
                    ids::enumerate(cx, values, "intltz_create_time_zone_id_enumeration", true)
                },
            ),
            "IntlIterator|false",
        ),
    ] {
        registry.function(entry);
        registry.describe_function(entry.name, return_type, None);
    }
}
