//! PHP's date extension: `date_default_timezone_get()` starts at `UTC` per request.

pub mod calendar;
pub mod zone;

use phera_extension_api::{Extension, FunctionEntry, Param, ParamType, Registry};

pub struct Date;

impl Extension for Date {
    fn name(&self) -> &'static str {
        "date"
    }

    fn register(&self, registry: &mut Registry) {
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
