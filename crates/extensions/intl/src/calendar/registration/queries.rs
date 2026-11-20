//! Query locale and calendar data without constructing a PHP object.

use phera_extension_api::Value;

pub(super) fn now() -> Value {
    Value::Float(crate::timezone::clock_millis() as f64)
}

pub(super) fn locales() -> Value {
    use phera_extension_api::{ArrayKey, PhpArray};
    let values: PhpArray = include_str!("../../../data/cldr-locales.txt")
        .lines()
        .enumerate()
        .map(|(i, name)| (ArrayKey::Int(i as i64), Value::from(name)))
        .collect();
    Value::from(values)
}

pub(super) fn keywords(
    context: &mut dyn phera_extension_api::Context,
    values: &[Value],
    site: &str,
) -> phera_extension_api::NativeResult {
    use phera_extension_api::args;
    if !args::bytes(values, 0).eq_ignore_ascii_case(b"calendar") {
        return crate::error::fail(context, site, 1, "Invalid keyword", Value::Bool(false));
    }
    let requested = crate::locale_id::cldr(&String::from_utf8_lossy(args::bytes(values, 1)));
    let kind = icu_calendar::AnyCalendarKind::try_new(requested.into())
        .unwrap_or(icu_calendar::AnyCalendarKind::Gregorian);
    let default = super::super::methods::kind(kind);
    let full = !args::bool(values, 2).unwrap_or(false);
    let others = [
        "gregorian",
        "japanese",
        "buddhist",
        "roc",
        "persian",
        "islamic",
        "islamic-civil",
        "hebrew",
        "chinese",
        "indian",
        "coptic",
        "ethiopic",
        "dangi",
    ];
    let values = std::iter::once(default)
        .chain(others.into_iter().filter(|name| full && *name != default))
        .map(Value::from);
    crate::iterator::values(context, values)
}
