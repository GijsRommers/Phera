//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M50: MethodEntry = MethodEntry::public_async(
    "expire",
    &[
        Param::new("key", ParamType::String),
        Param::new("timeout", ParamType::Int),
        Param::new("mode", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("expire", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::expire", c, p, a));
pub const M51: MethodEntry = MethodEntry::public_async(
    "expireAt",
    &[
        Param::new("key", ParamType::String),
        Param::new("timestamp", ParamType::Int),
        Param::new("mode", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("expireat", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::expireAt", c, p, a));
pub const M52: MethodEntry = MethodEntry::public_async(
    "failover",
    &[
        Param::new("to", ParamType::Array).nullable().optional(),
        Param::new("abort", ParamType::Bool).optional(),
        Param::new("timeout", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("failover", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::failover", c, p, a));
pub const M53: MethodEntry = MethodEntry::public_async(
    "expiretime",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("expiretime", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::expiretime", c, p, a));
pub const M54: MethodEntry = MethodEntry::public_async(
    "pexpiretime",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("pexpiretime", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::pexpiretime", c, p, a));
pub const M55: MethodEntry = MethodEntry::public_async(
    "fcall",
    &[
        Param::new("fn", ParamType::String),
        Param::new("keys", ParamType::Array).optional(),
        Param::new("args", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("fcall", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::fcall", c, p, a));
pub const M56: MethodEntry = MethodEntry::public_async(
    "fcall_ro",
    &[
        Param::new("fn", ParamType::String),
        Param::new("keys", ParamType::Array).optional(),
        Param::new("args", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("fcall_ro", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::fcall_ro", c, p, a));
pub const M57: MethodEntry = MethodEntry::public_async(
    "flushAll",
    &[Param::new("sync", ParamType::Bool).nullable().optional()],
    |c, o, a| dispatch("flushall", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::flushAll", c, p, a));
pub const M58: MethodEntry = MethodEntry::public_async(
    "flushDB",
    &[Param::new("sync", ParamType::Bool).nullable().optional()],
    |c, o, a| dispatch("flushdb", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::flushDB", c, p, a));
pub const M59: MethodEntry = MethodEntry::public_async(
    "function",
    &[
        Param::new("operation", ParamType::String),
        Param::new("args", ParamType::Mixed).optional().variadic(),
    ],
    |c, o, a| dispatch("function", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::function", c, p, a));
