//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M30: MethodEntry = MethodEntry::public_async(
    "command",
    &[
        Param::new("opt", ParamType::String).nullable().optional(),
        Param::new("args", ParamType::Mixed).optional().variadic(),
    ],
    |c, o, a| dispatch("command", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::command", c, p, a));
pub const M31: MethodEntry = MethodEntry::public_async(
    "config",
    &[
        Param::new("operation", ParamType::String),
        Param::new("key_or_settings", ParamType::ArrayOrString)
            .nullable()
            .optional(),
        Param::new("value", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("config", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::config", c, p, a));
pub const M32: MethodEntry = MethodEntry::public_async(
    "connect",
    &[
        Param::new("host", ParamType::String),
        Param::new("port", ParamType::Int).optional(),
        Param::new("timeout", ParamType::Float).optional(),
        Param::new("persistent_id", ParamType::String)
            .nullable()
            .optional(),
        Param::new("retry_interval", ParamType::Int).optional(),
        Param::new("read_timeout", ParamType::Float).optional(),
        Param::new("context", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("connect", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::connect", c, p, a));
pub const M33: MethodEntry = MethodEntry::public_async(
    "copy",
    &[
        Param::new("src", ParamType::String),
        Param::new("dst", ParamType::String),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("copy", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::copy", c, p, a));
pub const M34: MethodEntry =
    MethodEntry::public_async("dbSize", &[], |c, o, a| dispatch("dbsize", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::dbSize", c, p, a));
pub const M35: MethodEntry = MethodEntry::public_async(
    "debug",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("debug", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::debug", c, p, a));
pub const M36: MethodEntry = MethodEntry::public_async(
    "decr",
    &[
        Param::new("key", ParamType::String),
        Param::new("by", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("decr", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::decr", c, p, a));
pub const M37: MethodEntry = MethodEntry::public_async(
    "decrBy",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Int),
    ],
    |c, o, a| dispatch("decrby", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::decrBy", c, p, a));
pub const M38: MethodEntry = MethodEntry::public_async(
    "del",
    &[
        Param::new("key", ParamType::ArrayOrString),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("del", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::del", c, p, a));
pub const M39: MethodEntry = MethodEntry::public_async(
    "delifeq",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("delifeq", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::delifeq", c, p, a));
