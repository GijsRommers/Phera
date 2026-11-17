//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M60: MethodEntry = MethodEntry::public_async(
    "geoadd",
    &[
        Param::new("key", ParamType::String),
        Param::new("lng", ParamType::Float),
        Param::new("lat", ParamType::Float),
        Param::new("member", ParamType::String),
        Param::new("other_triples_and_options", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("geoadd", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::geoadd", c, p, a));
pub const M61: MethodEntry = MethodEntry::public_async(
    "geodist",
    &[
        Param::new("key", ParamType::String),
        Param::new("src", ParamType::String),
        Param::new("dst", ParamType::String),
        Param::new("unit", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("geodist", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::geodist", c, p, a));
pub const M62: MethodEntry = MethodEntry::public_async(
    "geohash",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::String),
        Param::new("other_members", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("geohash", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::geohash", c, p, a));
pub const M63: MethodEntry = MethodEntry::public_async(
    "geopos",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::String),
        Param::new("other_members", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("geopos", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::geopos", c, p, a));
pub const M64: MethodEntry = MethodEntry::public_async(
    "georadius",
    &[
        Param::new("key", ParamType::String),
        Param::new("lng", ParamType::Float),
        Param::new("lat", ParamType::Float),
        Param::new("radius", ParamType::Float),
        Param::new("unit", ParamType::String),
        Param::new("options", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("georadius", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::georadius", c, p, a));
pub const M65: MethodEntry = MethodEntry::public_async(
    "georadius_ro",
    &[
        Param::new("key", ParamType::String),
        Param::new("lng", ParamType::Float),
        Param::new("lat", ParamType::Float),
        Param::new("radius", ParamType::Float),
        Param::new("unit", ParamType::String),
        Param::new("options", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("georadius_ro", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::georadius_ro", c, p, a));
pub const M66: MethodEntry = MethodEntry::public_async(
    "georadiusbymember",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::String),
        Param::new("radius", ParamType::Float),
        Param::new("unit", ParamType::String),
        Param::new("options", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("georadiusbymember", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::georadiusbymember", c, p, a));
pub const M67: MethodEntry = MethodEntry::public_async(
    "georadiusbymember_ro",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::String),
        Param::new("radius", ParamType::Float),
        Param::new("unit", ParamType::String),
        Param::new("options", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("georadiusbymember_ro", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::georadiusbymember_ro", c, p, a));
pub const M68: MethodEntry = MethodEntry::public_async(
    "geosearch",
    &[
        Param::new("key", ParamType::String),
        Param::new("position", ParamType::ArrayOrString),
        Param::new("shape", ParamType::ArrayOrNumber),
        Param::new("unit", ParamType::String),
        Param::new("options", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("geosearch", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::geosearch", c, p, a));
pub const M69: MethodEntry = MethodEntry::public_async(
    "geosearchstore",
    &[
        Param::new("dst", ParamType::String),
        Param::new("src", ParamType::String),
        Param::new("position", ParamType::ArrayOrString),
        Param::new("shape", ParamType::ArrayOrNumber),
        Param::new("unit", ParamType::String),
        Param::new("options", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("geosearchstore", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::geosearchstore", c, p, a));
