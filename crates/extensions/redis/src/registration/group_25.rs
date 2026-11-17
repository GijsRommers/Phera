//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M250: MethodEntry = MethodEntry::public_async(
    "vrange",
    &[
        Param::new("key", ParamType::String),
        Param::new("min", ParamType::String),
        Param::new("max", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("vrange", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::vrange", c, p, a));
pub const M251: MethodEntry = MethodEntry::public_async(
    "vrem",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
    ],
    |c, o, a| dispatch("vrem", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::vrem", c, p, a));
pub const M252: MethodEntry = MethodEntry::public_async(
    "vsetattr",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
        Param::new("attributes", ParamType::ArrayOrString),
    ],
    |c, o, a| dispatch("vsetattr", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::vsetattr", c, p, a));
pub const M253: MethodEntry = MethodEntry::public_async(
    "vgetattr",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
        Param::new("decode", ParamType::Bool).optional(),
    ],
    |c, o, a| dispatch("vgetattr", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::vgetattr", c, p, a));
pub const M254: MethodEntry = MethodEntry::public_async(
    "vlinks",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
        Param::new("withscores", ParamType::Bool).optional(),
    ],
    |c, o, a| dispatch("vlinks", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::vlinks", c, p, a));
pub const M255: MethodEntry = MethodEntry::public_async(
    "xtrim",
    &[
        Param::new("key", ParamType::String),
        Param::new("threshold", ParamType::String),
        Param::new("approx", ParamType::Bool).optional(),
        Param::new("minid", ParamType::Bool).optional(),
        Param::new("limit", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("xtrim", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::xtrim", c, p, a));
pub const M256: MethodEntry = MethodEntry::public_async(
    "zAdd",
    &[
        Param::new("key", ParamType::String),
        Param::new("score_or_options", ParamType::ArrayOrFloat),
        Param::new("more_scores_and_mems", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("zadd", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zAdd", c, p, a));
pub const M257: MethodEntry = MethodEntry::public_async(
    "zCard",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("zcard", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zCard", c, p, a));
pub const M258: MethodEntry = MethodEntry::public_async(
    "zCount",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::IntOrString),
        Param::new("end", ParamType::IntOrString),
    ],
    |c, o, a| dispatch("zcount", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zCount", c, p, a));
pub const M259: MethodEntry = MethodEntry::public_async(
    "zIncrBy",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Float),
        Param::new("member", ParamType::Mixed),
    ],
    |c, o, a| dispatch("zincrby", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zIncrBy", c, p, a));
