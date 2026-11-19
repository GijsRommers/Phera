//! PHP's date extension: `date_default_timezone_get()` starts at `UTC` per request.

pub mod calendar;
mod classes;
mod format;
mod functions;
mod normalize;
mod parse;
mod parse_format;
mod posix;
mod register;
pub mod resolve;
mod sun;
pub mod zone;

use phera_extension_api::{Extension, FunctionEntry, Param, ParamType, Registry};

pub use classes::{Instant as DateTimeValue, ZoneKind, parse_timezone, read_timezone};

pub struct Date;

impl Extension for Date {
    fn name(&self) -> &'static str {
        "date"
    }

    fn register(&self, registry: &mut Registry) {
        registry.ini_update("date.timezone", |context, name, value, function| {
            Ok(zone::update_ini(context, name, value, function))
        });
        registry.deprecated_constant("DATE_RFC7231", classes::RFC7231_FORMAT);
        register::functions(registry);
        classes::register(registry);
        registry.function(FunctionEntry::sync(
            "date_default_timezone_get",
            &[],
            zone::get,
        ));
        const ZONE: &[Param] = &[Param::new("timezoneId", ParamType::String)];
        registry.function(FunctionEntry::sync(
            "date_default_timezone_set",
            ZONE,
            zone::set,
        ));
        registry.function(FunctionEntry::sync(
            "timezone_version_get",
            &[],
            zone::version,
        ));
    }
}
