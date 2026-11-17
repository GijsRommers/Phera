//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M120: MethodEntry = MethodEntry::public_async(
    "hscan",
    &[
        Param::new("key", ParamType::String),
        Param::new("iterator", ParamType::IntOrString)
            .nullable()
            .by_ref(),
        Param::new("pattern", ParamType::String)
            .nullable()
            .optional(),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("hscan", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hscan", c, p, a));
pub const M121: MethodEntry = MethodEntry::public_async(
    "expiremember",
    &[
        Param::new("key", ParamType::String),
        Param::new("field", ParamType::String),
        Param::new("ttl", ParamType::Int),
        Param::new("unit", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("expiremember", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::expiremember", c, p, a));
pub const M122: MethodEntry = MethodEntry::public_async(
    "expirememberat",
    &[
        Param::new("key", ParamType::String),
        Param::new("field", ParamType::String),
        Param::new("timestamp", ParamType::Int),
    ],
    |c, o, a| dispatch("expirememberat", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::expirememberat", c, p, a));
pub const M123: MethodEntry = MethodEntry::public_async(
    "incr",
    &[
        Param::new("key", ParamType::String),
        Param::new("by", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("incr", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::incr", c, p, a));
pub const M124: MethodEntry = MethodEntry::public_async(
    "incrBy",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Int),
    ],
    |c, o, a| dispatch("incrby", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::incrBy", c, p, a));
pub const M125: MethodEntry = MethodEntry::public_async(
    "incrByFloat",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Float),
    ],
    |c, o, a| dispatch("incrbyfloat", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::incrByFloat", c, p, a));
pub const M126: MethodEntry = MethodEntry::public_async(
    "info",
    &[Param::new("sections", ParamType::String)
        .optional()
        .variadic()],
    |c, o, a| dispatch("info", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::info", c, p, a));
pub const M127: MethodEntry = MethodEntry::public_async("isConnected", &[], |c, o, a| {
    dispatch("isconnected", c, o, a)
})
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::isConnected", c, p, a));
pub const M128: MethodEntry = MethodEntry::public_async(
    "keys",
    &[Param::new("pattern", ParamType::String)],
    |c, o, a| dispatch("keys", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::keys", c, p, a));
pub const M129: MethodEntry = MethodEntry::public_async(
    "lInsert",
    &[
        Param::new("key", ParamType::String),
        Param::new("pos", ParamType::String),
        Param::new("pivot", ParamType::Mixed),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("linsert", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::lInsert", c, p, a));
