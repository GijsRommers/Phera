//! `date_modify($date, '+1 day')` shares the mutable method's date arithmetic.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, Registry, Value, args};

use super::datetime_registration::{
    ABSOLUTE, CONSTRUCT, DATE, DAY, FORMAT, FROM_FORMAT, HOUR, MICROS, MINUTE, MODIFIER, MONTH,
    MUTABLE_DATE, SECOND, TARGET, TIMESTAMP, WEEK, WEEKDAY, YEAR, ZONE,
};
use super::instant::{IMMUTABLE, MUTABLE};
use super::{
    datetime, datetime_factory as factory, interval_arithmetic,
    interval_registration::{self, INTERVAL},
};

const DIFF: &[Param] = &[Param::new("baseObject", DATE.ty), TARGET, ABSOLUTE];

pub fn register(registry: &mut Registry) {
    for entry in [
        FunctionEntry::sync("date_create", CONSTRUCT, create_mutable),
        FunctionEntry::sync("date_create_immutable", CONSTRUCT, create_immutable),
        FunctionEntry::sync(
            "date_create_from_format",
            FROM_FORMAT,
            create_formatted_mutable,
        ),
        FunctionEntry::sync(
            "date_create_immutable_from_format",
            FROM_FORMAT,
            create_formatted_immutable,
        ),
        FunctionEntry::sync(
            "date_add",
            &[MUTABLE_DATE, INTERVAL],
            interval_registration::add,
        ),
        FunctionEntry::sync(
            "date_sub",
            &[MUTABLE_DATE, INTERVAL],
            interval_arithmetic::sub_function,
        ),
        FunctionEntry::sync("date_diff", DIFF, interval_registration::diff),
        FunctionEntry::sync("date_get_last_errors", &[], last_errors),
        FunctionEntry::sync("date_format", &[DATE, FORMAT], format),
        FunctionEntry::sync("date_modify", &[MUTABLE_DATE, MODIFIER], modify),
        FunctionEntry::sync("date_timezone_get", &[DATE], timezone),
        FunctionEntry::sync("date_timezone_set", &[MUTABLE_DATE, ZONE], set_zone),
        FunctionEntry::sync("date_offset_get", &[DATE], offset),
        FunctionEntry::sync(
            "date_time_set",
            &[MUTABLE_DATE, HOUR, MINUTE, SECOND, MICROS],
            set_time,
        ),
        FunctionEntry::sync("date_date_set", &[MUTABLE_DATE, YEAR, MONTH, DAY], set_date),
        FunctionEntry::sync(
            "date_isodate_set",
            &[MUTABLE_DATE, YEAR, WEEK, WEEKDAY],
            set_iso,
        ),
        FunctionEntry::sync(
            "date_timestamp_set",
            &[MUTABLE_DATE, TIMESTAMP],
            set_timestamp,
        ),
        FunctionEntry::sync("date_timestamp_get", &[DATE], timestamp),
    ] {
        registry.function(entry);
    }
}

fn create_mutable(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    factory::create(context, arguments, MUTABLE, "date_create", false)
}

fn create_immutable(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    factory::create(
        context,
        arguments,
        IMMUTABLE,
        "date_create_immutable",
        false,
    )
}

fn create_formatted_mutable(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    factory::create(context, arguments, MUTABLE, "date_create_from_format", true)
}

fn create_formatted_immutable(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    factory::create(
        context,
        arguments,
        IMMUTABLE,
        "date_create_immutable_from_format",
        true,
    )
}

fn last_errors(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    factory::last_errors(context, None, arguments)
}

fn format(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    datetime::format(context, args::object(arguments, 0), &arguments[1..])
}

fn modify(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    datetime::modify_with_site(
        context,
        args::object(arguments, 0),
        &arguments[1..],
        "date_modify",
    )
}

fn timezone(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    datetime::timezone(context, args::object(arguments, 0), &arguments[1..])
}

fn set_zone(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    datetime::set_zone(context, args::object(arguments, 0), &arguments[1..])
}

fn offset(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    datetime::offset(context, args::object(arguments, 0), &arguments[1..])
}

fn set_time(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    datetime::set_time(context, args::object(arguments, 0), &arguments[1..])
}

fn set_date(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    datetime::set_date(context, args::object(arguments, 0), &arguments[1..])
}

fn set_iso(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    datetime::set_iso(context, args::object(arguments, 0), &arguments[1..])
}

fn set_timestamp(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    datetime::set_timestamp(context, args::object(arguments, 0), &arguments[1..])
}

fn timestamp(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    datetime::timestamp(context, args::object(arguments, 0), &arguments[1..])
}
