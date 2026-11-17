//! Register PHP's procedural signatures, including nullable timestamp defaults.
//! `date('c', null)` uses the request's current clock just like `date('c')`.

use crate::functions;
use phera_extension_api::{FunctionEntry, Param, ParamType, Registry};

pub fn functions(registry: &mut Registry) {
    // Procedural format constants are also used by parser round-trip tests.
    for (name, format) in [
        ("DATE_ATOM", "Y-m-d\\TH:i:sP"),
        ("DATE_COOKIE", "l, d-M-Y H:i:s T"),
        ("DATE_ISO8601", "Y-m-d\\TH:i:sO"),
        ("DATE_ISO8601_EXPANDED", "X-m-d\\TH:i:sP"),
        ("DATE_RFC822", "D, d M y H:i:s O"),
        ("DATE_RFC850", "l, d-M-y H:i:s T"),
        ("DATE_RFC1036", "D, d M y H:i:s O"),
        ("DATE_RFC1123", "D, d M Y H:i:s O"),
        ("DATE_RFC2822", "D, d M Y H:i:s O"),
        ("DATE_RFC3339", "Y-m-d\\TH:i:sP"),
        ("DATE_RFC3339_EXTENDED", "Y-m-d\\TH:i:s.vP"),
        ("DATE_RSS", "D, d M Y H:i:s O"),
        ("DATE_W3C", "Y-m-d\\TH:i:sP"),
    ] {
        registry.constant(name, format);
    }
    const STRING: &[Param] = &[Param::new("datetime", ParamType::String)];
    const STRTOTIME: &[Param] = &[
        Param::new("datetime", ParamType::String),
        Param::new("baseTimestamp", ParamType::Int)
            .nullable()
            .optional(),
    ];
    registry.function(FunctionEntry::sync(
        "date_parse",
        STRING,
        crate::parse::date_parse,
    ));
    registry.function(FunctionEntry::sync(
        "strtotime",
        STRTOTIME,
        crate::parse::strtotime,
    ));
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
    const SUN: &[Param] = &[
        Param::new("timestamp", ParamType::Int),
        Param::new("latitude", ParamType::Float),
        Param::new("longitude", ParamType::Float),
    ];
    for entry in [
        FunctionEntry::sync("date_sun_info", SUN, crate::sun::info),
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
