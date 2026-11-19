//! Procedural calendar aliases use the same handlers as the class methods.

use phera_extension_api::{FunctionEntry, Registry};

use super::{Factory, Op, create, invoke, keywords, locales, now, parameters::*};
use crate::unsupported;

mod settings;

pub fn register(registry: &mut Registry) {
    settings::register(registry);
    for (entry, result) in [
        (
            FunctionEntry::sync("intlcal_create_instance", CREATE, |cx, values| {
                create(
                    cx,
                    None,
                    values,
                    "intlcal_create_instance",
                    Factory::Instance,
                )
            }),
            "?IntlCalendar",
        ),
        (
            FunctionEntry::sync("intlcal_from_date_time", FROM_DATE, |cx, values| {
                create(
                    cx,
                    None,
                    values,
                    "intlcal_from_date_time",
                    Factory::FromDate,
                )
            }),
            "?IntlCalendar",
        ),
        (
            FunctionEntry::sync("intlgregcal_create_instance", GREG_CREATE, |cx, values| {
                create(
                    cx,
                    None,
                    values,
                    "intlgregcal_create_instance",
                    Factory::Gregorian,
                )
            }),
            "?IntlGregorianCalendar",
        ),
        (
            FunctionEntry::sync("intlcal_get_now", &[], |_, _| Ok(now())),
            "float",
        ),
        (
            FunctionEntry::sync("intlcal_get_available_locales", &[], |_, _| Ok(locales())),
            "array",
        ),
        (
            FunctionEntry::sync(
                "intlcal_get_keyword_values_for_locale",
                KEYWORDS,
                |cx, values| keywords(cx, values, "intlcal_get_keyword_values_for_locale"),
            ),
            "IntlIterator|false",
        ),
        (
            FunctionEntry::sync("intlcal_field_difference", FN_DIFFERENCE, |_, _| {
                Err(unsupported::item("intlcal_field_difference"))
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("intlcal_get", FN_FIELD, |cx, values| {
                invoke(cx, None, values, "intlcal_get", Op::Get)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("intlcal_add", FN_ALTER, |cx, values| {
                invoke(cx, None, values, "intlcal_add", Op::Add)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("intlcal_roll", FN_ROLL, |cx, values| {
                invoke(cx, None, values, "intlcal_roll", Op::Roll)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("intlcal_set", FN_SET, |cx, values| {
                invoke(cx, None, values, "intlcal_set", Op::Set)
            }),
            "true",
        ),
        (
            FunctionEntry::sync("intlcal_get_time", INSTANCE, |cx, values| {
                invoke(cx, None, values, "intlcal_get_time", Op::Time)
            }),
            "float|false",
        ),
        (
            FunctionEntry::sync("intlcal_set_time", FN_STAMP, |cx, values| {
                invoke(cx, None, values, "intlcal_set_time", Op::SetTime)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("intlcal_get_time_zone", INSTANCE, |cx, values| {
                invoke(cx, None, values, "intlcal_get_time_zone", Op::TimeZone)
            }),
            "IntlTimeZone|false",
        ),
        (
            FunctionEntry::sync("intlcal_set_time_zone", FN_ZONE, |cx, values| {
                invoke(cx, None, values, "intlcal_set_time_zone", Op::SetTimeZone)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("intlcal_to_date_time", INSTANCE, |cx, values| {
                invoke(cx, None, values, "intlcal_to_date_time", Op::ToDate)
            }),
            "DateTime|false",
        ),
        (
            FunctionEntry::sync("intlcal_get_first_day_of_week", INSTANCE, |cx, values| {
                invoke(cx, None, values, "intlcal_get_first_day_of_week", Op::First)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("intlcal_set_first_day_of_week", FN_DAY, |cx, values| {
                invoke(
                    cx,
                    None,
                    values,
                    "intlcal_set_first_day_of_week",
                    Op::SetFirst,
                )
            }),
            "true",
        ),
        (
            FunctionEntry::sync(
                "intlcal_get_minimal_days_in_first_week",
                INSTANCE,
                |cx, values| {
                    invoke(
                        cx,
                        None,
                        values,
                        "intlcal_get_minimal_days_in_first_week",
                        Op::Minimal,
                    )
                },
            ),
            "int|false",
        ),
        (
            FunctionEntry::sync(
                "intlcal_set_minimal_days_in_first_week",
                FN_DAYS,
                |cx, values| {
                    invoke(
                        cx,
                        None,
                        values,
                        "intlcal_set_minimal_days_in_first_week",
                        Op::SetMinimal,
                    )
                },
            ),
            "true",
        ),
        (
            FunctionEntry::sync("intlcal_is_weekend", FN_WEEKEND, |cx, values| {
                invoke(cx, None, values, "intlcal_is_weekend", Op::Weekend)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("intlcal_get_day_of_week_type", FN_DAY, |cx, values| {
                invoke(
                    cx,
                    None,
                    values,
                    "intlcal_get_day_of_week_type",
                    Op::DayType,
                )
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("intlcal_get_weekend_transition", FN_DAY, |cx, values| {
                invoke(
                    cx,
                    None,
                    values,
                    "intlcal_get_weekend_transition",
                    Op::WeekendTransition,
                )
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("intlcal_before", FN_OTHER, |cx, values| {
                invoke(cx, None, values, "intlcal_before", Op::Before)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("intlcal_after", FN_OTHER, |cx, values| {
                invoke(cx, None, values, "intlcal_after", Op::After)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("intlcal_equals", FN_OTHER, |cx, values| {
                invoke(cx, None, values, "intlcal_equals", Op::Equals)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("intlcal_is_equivalent_to", FN_OTHER, |cx, values| {
                invoke(cx, None, values, "intlcal_is_equivalent_to", Op::Equivalent)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("intlcal_is_lenient", INSTANCE, |cx, values| {
                invoke(cx, None, values, "intlcal_is_lenient", Op::Lenient)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("intlcal_set_lenient", FN_LENIENT, |cx, values| {
                invoke(cx, None, values, "intlcal_set_lenient", Op::SetLenient)
            }),
            "true",
        ),
        (
            FunctionEntry::sync("intlcal_get_locale", FN_TYPE, |cx, values| {
                invoke(cx, None, values, "intlcal_get_locale", Op::Locale)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("intlcal_get_type", INSTANCE, |cx, values| {
                invoke(cx, None, values, "intlcal_get_type", Op::Kind)
            }),
            "string",
        ),
        (
            FunctionEntry::sync("intlcal_in_daylight_time", INSTANCE, |cx, values| {
                invoke(cx, None, values, "intlcal_in_daylight_time", Op::Dst)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("intlcal_clear", FN_CLEAR, |cx, values| {
                invoke(cx, None, values, "intlcal_clear", Op::Clear)
            }),
            "true",
        ),
        (
            FunctionEntry::sync("intlcal_is_set", FN_FIELD, |cx, values| {
                invoke(cx, None, values, "intlcal_is_set", Op::IsSet)
            }),
            "bool",
        ),
    ] {
        registry.function(entry);
        registry.describe_function(entry.name, result, None);
    }
}
