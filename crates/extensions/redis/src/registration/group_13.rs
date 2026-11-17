//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M130: MethodEntry = MethodEntry::public_async(
    "lLen",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("llen", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::lLen", c, p, a));
pub const M131: MethodEntry = MethodEntry::public_async(
    "lMove",
    &[
        Param::new("src", ParamType::String),
        Param::new("dst", ParamType::String),
        Param::new("wherefrom", ParamType::String),
        Param::new("whereto", ParamType::String),
    ],
    |c, o, a| dispatch("lmove", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::lMove", c, p, a));
pub const M132: MethodEntry = MethodEntry::public_async(
    "blmove",
    &[
        Param::new("src", ParamType::String),
        Param::new("dst", ParamType::String),
        Param::new("wherefrom", ParamType::String),
        Param::new("whereto", ParamType::String),
        Param::new("timeout", ParamType::Float),
    ],
    |c, o, a| dispatch("blmove", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::blmove", c, p, a));
pub const M133: MethodEntry = MethodEntry::public_async(
    "lPop",
    &[
        Param::new("key", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("lpop", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::lPop", c, p, a));
pub const M134: MethodEntry = MethodEntry::public_async(
    "lPos",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("lpos", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::lPos", c, p, a));
pub const M135: MethodEntry = MethodEntry::public_async(
    "lPush",
    &[
        Param::new("key", ParamType::String),
        Param::new("elements", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("lpush", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::lPush", c, p, a));
pub const M136: MethodEntry = MethodEntry::public_async(
    "rPush",
    &[
        Param::new("key", ParamType::String),
        Param::new("elements", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("rpush", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::rPush", c, p, a));
pub const M137: MethodEntry = MethodEntry::public_async(
    "lPushx",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("lpushx", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::lPushx", c, p, a));
pub const M138: MethodEntry = MethodEntry::public_async(
    "rPushx",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("rpushx", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::rPushx", c, p, a));
pub const M139: MethodEntry = MethodEntry::public_async(
    "lSet",
    &[
        Param::new("key", ParamType::String),
        Param::new("index", ParamType::Int),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("lset", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::lSet", c, p, a));
