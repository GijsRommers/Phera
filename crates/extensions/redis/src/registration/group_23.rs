//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M230: MethodEntry = MethodEntry::public_async(
    "xadd",
    &[
        Param::new("key", ParamType::String),
        Param::new("id", ParamType::String),
        Param::new("values", ParamType::Array),
        Param::new("maxlen", ParamType::Int).optional(),
        Param::new("approx", ParamType::Bool).optional(),
        Param::new("nomkstream", ParamType::Bool).optional(),
    ],
    |c, o, a| dispatch("xadd", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::xadd", c, p, a));
pub const M231: MethodEntry = MethodEntry::public_async(
    "xautoclaim",
    &[
        Param::new("key", ParamType::String),
        Param::new("group", ParamType::String),
        Param::new("consumer", ParamType::String),
        Param::new("min_idle", ParamType::Int),
        Param::new("start", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
        Param::new("justid", ParamType::Bool).optional(),
    ],
    |c, o, a| dispatch("xautoclaim", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::xautoclaim", c, p, a));
pub const M232: MethodEntry = MethodEntry::public_async(
    "xclaim",
    &[
        Param::new("key", ParamType::String),
        Param::new("group", ParamType::String),
        Param::new("consumer", ParamType::String),
        Param::new("min_idle", ParamType::Int),
        Param::new("ids", ParamType::Array),
        Param::new("options", ParamType::Array),
    ],
    |c, o, a| dispatch("xclaim", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::xclaim", c, p, a));
pub const M233: MethodEntry = MethodEntry::public_async(
    "xdel",
    &[
        Param::new("key", ParamType::String),
        Param::new("ids", ParamType::Array),
    ],
    |c, o, a| dispatch("xdel", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::xdel", c, p, a));
pub const M234: MethodEntry = MethodEntry::public_async(
    "xgroup",
    &[
        Param::new("operation", ParamType::String),
        Param::new("key", ParamType::String).nullable().optional(),
        Param::new("group", ParamType::String).nullable().optional(),
        Param::new("id_or_consumer", ParamType::String)
            .nullable()
            .optional(),
        Param::new("mkstream", ParamType::Bool).optional(),
        Param::new("entries_read", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("xgroup", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::xgroup", c, p, a));
pub const M235: MethodEntry = MethodEntry::public_async(
    "xinfo",
    &[
        Param::new("operation", ParamType::String),
        Param::new("arg1", ParamType::String).nullable().optional(),
        Param::new("arg2", ParamType::String).nullable().optional(),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("xinfo", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::xinfo", c, p, a));
pub const M236: MethodEntry = MethodEntry::public_async(
    "xlen",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("xlen", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::xlen", c, p, a));
pub const M237: MethodEntry = MethodEntry::public_async(
    "xpending",
    &[
        Param::new("key", ParamType::String),
        Param::new("group", ParamType::String),
        Param::new("start", ParamType::String).nullable().optional(),
        Param::new("end", ParamType::String).nullable().optional(),
        Param::new("count", ParamType::Int).optional(),
        Param::new("consumer", ParamType::String)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("xpending", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::xpending", c, p, a));
pub const M238: MethodEntry = MethodEntry::public_async(
    "xrange",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::String),
        Param::new("end", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("xrange", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::xrange", c, p, a));
pub const M239: MethodEntry = MethodEntry::public_async(
    "xread",
    &[
        Param::new("streams", ParamType::Array),
        Param::new("count", ParamType::Int).optional(),
        Param::new("block", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("xread", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::xread", c, p, a));
