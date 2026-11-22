//! `nl_langinfo(CODESET)` describes the request locale. Locale strings use C/POSIX tables.

use nix::libc;
use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};

const ITEMS: &[(&str, libc::nl_item, &str)] = &[
    ("ABDAY_1", libc::ABDAY_1, "Sun"),
    ("ABDAY_2", libc::ABDAY_2, "Mon"),
    ("ABDAY_3", libc::ABDAY_3, "Tue"),
    ("ABDAY_4", libc::ABDAY_4, "Wed"),
    ("ABDAY_5", libc::ABDAY_5, "Thu"),
    ("ABDAY_6", libc::ABDAY_6, "Fri"),
    ("ABDAY_7", libc::ABDAY_7, "Sat"),
    ("DAY_1", libc::DAY_1, "Sunday"),
    ("DAY_2", libc::DAY_2, "Monday"),
    ("DAY_3", libc::DAY_3, "Tuesday"),
    ("DAY_4", libc::DAY_4, "Wednesday"),
    ("DAY_5", libc::DAY_5, "Thursday"),
    ("DAY_6", libc::DAY_6, "Friday"),
    ("DAY_7", libc::DAY_7, "Saturday"),
    ("ABMON_1", libc::ABMON_1, "Jan"),
    ("ABMON_2", libc::ABMON_2, "Feb"),
    ("ABMON_3", libc::ABMON_3, "Mar"),
    ("ABMON_4", libc::ABMON_4, "Apr"),
    ("ABMON_5", libc::ABMON_5, "May"),
    ("ABMON_6", libc::ABMON_6, "Jun"),
    ("ABMON_7", libc::ABMON_7, "Jul"),
    ("ABMON_8", libc::ABMON_8, "Aug"),
    ("ABMON_9", libc::ABMON_9, "Sep"),
    ("ABMON_10", libc::ABMON_10, "Oct"),
    ("ABMON_11", libc::ABMON_11, "Nov"),
    ("ABMON_12", libc::ABMON_12, "Dec"),
    ("MON_1", libc::MON_1, "January"),
    ("MON_2", libc::MON_2, "February"),
    ("MON_3", libc::MON_3, "March"),
    ("MON_4", libc::MON_4, "April"),
    ("MON_5", libc::MON_5, "May"),
    ("MON_6", libc::MON_6, "June"),
    ("MON_7", libc::MON_7, "July"),
    ("MON_8", libc::MON_8, "August"),
    ("MON_9", libc::MON_9, "September"),
    ("MON_10", libc::MON_10, "October"),
    ("MON_11", libc::MON_11, "November"),
    ("MON_12", libc::MON_12, "December"),
    ("AM_STR", libc::AM_STR, "AM"),
    ("PM_STR", libc::PM_STR, "PM"),
    ("D_T_FMT", libc::D_T_FMT, "%a %b %e %H:%M:%S %Y"),
    ("D_FMT", libc::D_FMT, "%m/%d/%y"),
    ("T_FMT", libc::T_FMT, "%H:%M:%S"),
    ("T_FMT_AMPM", libc::T_FMT_AMPM, "%I:%M:%S %p"),
    ("ERA", libc::ERA, ""),
    ("ERA_D_FMT", libc::ERA_D_FMT, ""),
    ("ERA_D_T_FMT", libc::ERA_D_T_FMT, ""),
    ("ERA_T_FMT", libc::ERA_T_FMT, ""),
    ("ALT_DIGITS", libc::ALT_DIGITS, ""),
    ("RADIXCHAR", libc::RADIXCHAR, "."),
    ("THOUSEP", libc::THOUSEP, ""),
    ("CRNCYSTR", libc::CRNCYSTR, ""),
    ("YESEXPR", libc::YESEXPR, "^[yY]"),
    ("NOEXPR", libc::NOEXPR, "^[nN]"),
];

pub(super) fn register(registry: &mut Registry) {
    registry.constant("CODESET", i64::from(libc::CODESET));
    for &(name, id, _) in ITEMS {
        registry.constant(name, i64::from(id));
    }
    registry.function(FunctionEntry::sync(
        "nl_langinfo",
        const { &[Param::new("item", ParamType::Int)] },
        info,
    ));
    registry.describe_function("nl_langinfo", "string|false", Some("standard"));
}

fn info(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let item = args::int(arguments, 0).unwrap_or_default();
    if item == i64::from(libc::CODESET) {
        let locale = super::ctype_name(context);
        let utf8 = locale
            .as_bytes()
            .rsplit(|&byte| byte == b'.')
            .next()
            .is_some_and(|name| {
                name.eq_ignore_ascii_case(b"utf-8") || name.eq_ignore_ascii_case(b"utf8")
            });
        let codeset = if utf8 {
            "UTF-8"
        } else if cfg!(target_vendor = "apple") {
            "US-ASCII"
        } else {
            "ANSI_X3.4-1968"
        };
        return Ok(Value::from(codeset));
    }
    if let Some((_, _, value)) = ITEMS.iter().find(|(_, id, _)| i64::from(*id) == item) {
        return Ok(Value::from(*value));
    }
    context.warning(format!("nl_langinfo(): Item '{item}' is not valid"));
    Ok(Value::Bool(false))
}
