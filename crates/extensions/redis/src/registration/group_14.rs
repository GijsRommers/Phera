//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M140: MethodEntry =
    MethodEntry::public_async("lastSave", &[], |c, o, a| dispatch("lastsave", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::lastSave", c, p, a));
pub const M141: MethodEntry = MethodEntry::public_async(
    "lindex",
    &[
        Param::new("key", ParamType::String),
        Param::new("index", ParamType::Int),
    ],
    |c, o, a| dispatch("lindex", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::lindex", c, p, a));
pub const M142: MethodEntry = MethodEntry::public_async(
    "lrange",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::Int),
        Param::new("end", ParamType::Int),
    ],
    |c, o, a| dispatch("lrange", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::lrange", c, p, a));
pub const M143: MethodEntry = MethodEntry::public_async(
    "lrem",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("lrem", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::lrem", c, p, a));
pub const M144: MethodEntry = MethodEntry::public_async(
    "ltrim",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::Int),
        Param::new("end", ParamType::Int),
    ],
    |c, o, a| dispatch("ltrim", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::ltrim", c, p, a));
pub const M145: MethodEntry = MethodEntry::public_async(
    "mget",
    &[Param::new("keys", ParamType::Array)],
    |c, o, a| dispatch("mget", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::mget", c, p, a));
pub const M146: MethodEntry = MethodEntry::public_async(
    "migrate",
    &[
        Param::new("host", ParamType::String),
        Param::new("port", ParamType::Int),
        Param::new("key", ParamType::ArrayOrString),
        Param::new("dstdb", ParamType::Int),
        Param::new("timeout", ParamType::Int),
        Param::new("copy", ParamType::Bool).optional(),
        Param::new("replace", ParamType::Bool).optional(),
        Param::new("credentials", ParamType::Mixed).optional(),
    ],
    |c, o, a| dispatch("migrate", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::migrate", c, p, a));
pub const M147: MethodEntry = MethodEntry::public_async(
    "move",
    &[
        Param::new("key", ParamType::String),
        Param::new("index", ParamType::Int),
    ],
    |c, o, a| dispatch("move", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::move", c, p, a));
pub const M148: MethodEntry = MethodEntry::public_async(
    "mset",
    &[Param::new("key_values", ParamType::Array)],
    |c, o, a| dispatch("mset", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::mset", c, p, a));
pub const M149: MethodEntry = MethodEntry::public_async(
    "msetnx",
    &[Param::new("key_values", ParamType::Array)],
    |c, o, a| dispatch("msetnx", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::msetnx", c, p, a));
