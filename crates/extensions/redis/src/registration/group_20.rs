//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M200: MethodEntry = MethodEntry::public_async(
    "setOption",
    &[
        Param::new("option", ParamType::Int),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("setoption", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::setOption", c, p, a));
pub const M201: MethodEntry = MethodEntry::public_async(
    "setex",
    &[
        Param::new("key", ParamType::String),
        Param::new("expire", ParamType::Int),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("setex", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::setex", c, p, a));
pub const M202: MethodEntry = MethodEntry::public_async(
    "setnx",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("setnx", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::setnx", c, p, a));
pub const M203: MethodEntry = MethodEntry::public_async(
    "sismember",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("sismember", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sismember", c, p, a));
pub const M204: MethodEntry = MethodEntry::public_async(
    "slaveof",
    &[
        Param::new("host", ParamType::String).nullable().optional(),
        Param::new("port", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("slaveof", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::slaveof", c, p, a));
pub const M205: MethodEntry = MethodEntry::public_async(
    "replicaof",
    &[
        Param::new("host", ParamType::String).nullable().optional(),
        Param::new("port", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("replicaof", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::replicaof", c, p, a));
pub const M206: MethodEntry = MethodEntry::public_async(
    "touch",
    &[
        Param::new("key_or_array", ParamType::ArrayOrString),
        Param::new("more_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("touch", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::touch", c, p, a));
pub const M207: MethodEntry = MethodEntry::public_async(
    "slowlog",
    &[
        Param::new("operation", ParamType::String),
        Param::new("length", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("slowlog", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::slowlog", c, p, a));
pub const M208: MethodEntry = MethodEntry::public_async(
    "sort",
    &[
        Param::new("key", ParamType::String),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("sort", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sort", c, p, a));
pub const M209: MethodEntry = MethodEntry::public_async(
    "sort_ro",
    &[
        Param::new("key", ParamType::String),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("sort_ro", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sort_ro", c, p, a));
