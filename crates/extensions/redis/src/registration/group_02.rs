//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M20: MethodEntry = MethodEntry::public_async(
    "brpoplpush",
    &[
        Param::new("src", ParamType::String),
        Param::new("dst", ParamType::String),
        Param::new("timeout", ParamType::Number),
    ],
    |c, o, a| dispatch("brpoplpush", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::brpoplpush", c, p, a));
pub const M21: MethodEntry = MethodEntry::public_async(
    "bzPopMax",
    &[
        Param::new("key", ParamType::ArrayOrString),
        Param::new("timeout_or_key", ParamType::IntOrString),
        Param::new("extra_args", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("bzpopmax", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::bzPopMax", c, p, a));
pub const M22: MethodEntry = MethodEntry::public_async(
    "bzPopMin",
    &[
        Param::new("key", ParamType::ArrayOrString),
        Param::new("timeout_or_key", ParamType::IntOrString),
        Param::new("extra_args", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("bzpopmin", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::bzPopMin", c, p, a));
pub const M23: MethodEntry = MethodEntry::public_async(
    "bzmpop",
    &[
        Param::new("timeout", ParamType::Float),
        Param::new("keys", ParamType::Array),
        Param::new("from", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("bzmpop", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::bzmpop", c, p, a));
pub const M24: MethodEntry = MethodEntry::public_async(
    "zmpop",
    &[
        Param::new("keys", ParamType::Array),
        Param::new("from", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("zmpop", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zmpop", c, p, a));
pub const M25: MethodEntry = MethodEntry::public_async(
    "blmpop",
    &[
        Param::new("timeout", ParamType::Float),
        Param::new("keys", ParamType::Array),
        Param::new("from", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("blmpop", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::blmpop", c, p, a));
pub const M26: MethodEntry = MethodEntry::public_async(
    "lmpop",
    &[
        Param::new("keys", ParamType::Array),
        Param::new("from", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("lmpop", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::lmpop", c, p, a));
pub const M27: MethodEntry = MethodEntry::public_async("clearLastError", &[], |c, o, a| {
    dispatch("clearlasterror", c, o, a)
})
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::clearLastError", c, p, a));
pub const M28: MethodEntry = MethodEntry::public_async(
    "client",
    &[
        Param::new("opt", ParamType::String),
        Param::new("args", ParamType::Mixed).optional().variadic(),
    ],
    |c, o, a| dispatch("client", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::client", c, p, a));
pub const M29: MethodEntry =
    MethodEntry::public_async("close", &[], |c, o, a| dispatch("close", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::close", c, p, a));
