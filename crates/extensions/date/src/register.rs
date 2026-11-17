//! Register PHP's procedural signatures, including nullable timestamp defaults.
//! `date('c', null)` uses the request's current clock just like `date('c')`.

use crate::functions;
use phera_extension_api::{FunctionEntry, Param, ParamType, Registry};

pub fn functions(registry: &mut Registry) {
    const PARSE: &[Param] = &[
        Param::new("format", ParamType::String),
        Param::new("datetime", ParamType::String),
    ];
    registry.function(FunctionEntry::sync(
        "date_parse_from_format",
        PARSE,
        crate::parse_format::function,
    ));
    const FORMAT: &[Param] = &[
        Param::new("format", ParamType::String),
        Param::new("timestamp", ParamType::Int)
            .nullable()
            .optional(),
    ];
    const TIMESTAMP: &[Param] = &[Param::new("timestamp", ParamType::Int)
        .nullable()
        .optional()];
    const LOCAL: &[Param] = &[
        Param::new("timestamp", ParamType::Int)
            .nullable()
            .optional(),
        Param::new("associative", ParamType::Bool).optional(),
    ];
    const CHECK: &[Param] = &[
        Param::new("month", ParamType::Int),
        Param::new("day", ParamType::Int),
        Param::new("year", ParamType::Int),
    ];
    const MAKE: &[Param] = &[
        Param::new("hour", ParamType::Int),
        Param::new("minute", ParamType::Int).nullable().optional(),
        Param::new("second", ParamType::Int).nullable().optional(),
        Param::new("month", ParamType::Int).nullable().optional(),
        Param::new("day", ParamType::Int).nullable().optional(),
        Param::new("year", ParamType::Int).nullable().optional(),
    ];
    for entry in [
        FunctionEntry::sync("date", FORMAT, functions::date),
        FunctionEntry::sync("gmdate", FORMAT, functions::gmdate),
        FunctionEntry::sync("idate", FORMAT, functions::idate),
        FunctionEntry::sync("getdate", TIMESTAMP, functions::getdate),
        FunctionEntry::sync("localtime", LOCAL, functions::localtime),
        FunctionEntry::sync("checkdate", CHECK, functions::checkdate),
        FunctionEntry::sync("mktime", MAKE, functions::mktime),
        FunctionEntry::sync("gmmktime", MAKE, functions::gmmktime),
    ] {
        registry.function(entry);
    }
}
