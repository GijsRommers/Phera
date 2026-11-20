//! Procedural locale, calendar settings and Gregorian operations.

use phera_extension_api::{FunctionEntry, Registry};

use super::super::parameters::*;
use super::{Op, invoke};

pub(super) fn register(registry: &mut Registry) {
    for (entry, result) in [
        (
            FunctionEntry::sync("intlcal_get_minimum", FN_FIELD, |cx, values| {
                invoke(cx, None, values, "intlcal_get_minimum", Op::Minimum)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("intlcal_get_maximum", FN_FIELD, |cx, values| {
                invoke(cx, None, values, "intlcal_get_maximum", Op::Maximum)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("intlcal_get_actual_minimum", FN_FIELD, |cx, values| {
                invoke(
                    cx,
                    None,
                    values,
                    "intlcal_get_actual_minimum",
                    Op::ActualMin,
                )
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("intlcal_get_actual_maximum", FN_FIELD, |cx, values| {
                invoke(
                    cx,
                    None,
                    values,
                    "intlcal_get_actual_maximum",
                    Op::ActualMax,
                )
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("intlcal_get_greatest_minimum", FN_FIELD, |cx, values| {
                invoke(
                    cx,
                    None,
                    values,
                    "intlcal_get_greatest_minimum",
                    Op::GreatestMin,
                )
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("intlcal_get_least_maximum", FN_FIELD, |cx, values| {
                invoke(cx, None, values, "intlcal_get_least_maximum", Op::LeastMax)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("intlcal_get_error_code", INSTANCE, |cx, values| {
                invoke(cx, None, values, "intlcal_get_error_code", Op::ErrorCode)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("intlcal_get_error_message", INSTANCE, |cx, values| {
                invoke(
                    cx,
                    None,
                    values,
                    "intlcal_get_error_message",
                    Op::ErrorMessage,
                )
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync(
                "intlcal_get_repeated_wall_time_option",
                INSTANCE,
                |cx, values| {
                    invoke(
                        cx,
                        None,
                        values,
                        "intlcal_get_repeated_wall_time_option",
                        Op::Repeated,
                    )
                },
            ),
            "int",
        ),
        (
            FunctionEntry::sync(
                "intlcal_get_skipped_wall_time_option",
                INSTANCE,
                |cx, values| {
                    invoke(
                        cx,
                        None,
                        values,
                        "intlcal_get_skipped_wall_time_option",
                        Op::Skipped,
                    )
                },
            ),
            "int",
        ),
        (
            FunctionEntry::sync(
                "intlcal_set_repeated_wall_time_option",
                FN_OPTION,
                |cx, values| {
                    invoke(
                        cx,
                        None,
                        values,
                        "intlcal_set_repeated_wall_time_option",
                        Op::SetRepeated,
                    )
                },
            ),
            "true",
        ),
        (
            FunctionEntry::sync(
                "intlcal_set_skipped_wall_time_option",
                FN_OPTION,
                |cx, values| {
                    invoke(
                        cx,
                        None,
                        values,
                        "intlcal_set_skipped_wall_time_option",
                        Op::SetSkipped,
                    )
                },
            ),
            "true",
        ),
        (
            FunctionEntry::sync(
                "intlgregcal_get_gregorian_change",
                GREG_INSTANCE,
                |cx, values| {
                    invoke(
                        cx,
                        None,
                        values,
                        "intlgregcal_get_gregorian_change",
                        Op::GregorianChange,
                    )
                },
            ),
            "float",
        ),
        (
            FunctionEntry::sync(
                "intlgregcal_set_gregorian_change",
                GREG_STAMP,
                |cx, values| {
                    invoke(
                        cx,
                        None,
                        values,
                        "intlgregcal_set_gregorian_change",
                        Op::SetGregorianChange,
                    )
                },
            ),
            "bool",
        ),
        (
            FunctionEntry::sync("intlgregcal_is_leap_year", GREG_YEAR, |cx, values| {
                invoke(cx, None, values, "intlgregcal_is_leap_year", Op::Leap)
            }),
            "bool",
        ),
    ] {
        registry.function(entry);
        registry.describe_function(entry.name, result, None);
    }
}
