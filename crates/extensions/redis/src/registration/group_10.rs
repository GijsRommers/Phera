//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M100: MethodEntry = MethodEntry::public_async(
    "hLen",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("hlen", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hLen", c, p, a));
pub const M101: MethodEntry = MethodEntry::public_async(
    "hMget",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
    ],
    |c, o, a| dispatch("hmget", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hMget", c, p, a));
pub const M102: MethodEntry = MethodEntry::public_async(
    "hgetex",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
        Param::new("expiry", ParamType::ArrayOrString)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("hgetex", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hgetex", c, p, a));
pub const M103: MethodEntry = MethodEntry::public_async(
    "hsetex",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
        Param::new("expiry", ParamType::Array).nullable().optional(),
    ],
    |c, o, a| dispatch("hsetex", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hsetex", c, p, a));
pub const M104: MethodEntry = MethodEntry::public_async(
    "hgetdel",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
    ],
    |c, o, a| dispatch("hgetdel", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hgetdel", c, p, a));
pub const M105: MethodEntry = MethodEntry::public_async(
    "hMset",
    &[
        Param::new("key", ParamType::String),
        Param::new("fieldvals", ParamType::Array),
    ],
    |c, o, a| dispatch("hmset", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hMset", c, p, a));
pub const M106: MethodEntry = MethodEntry::public_async(
    "hRandField",
    &[
        Param::new("key", ParamType::String),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("hrandfield", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hRandField", c, p, a));
pub const M107: MethodEntry = MethodEntry::public_async(
    "hSet",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields_and_vals", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("hset", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hSet", c, p, a));
pub const M108: MethodEntry = MethodEntry::public_async(
    "hSetNx",
    &[
        Param::new("key", ParamType::String),
        Param::new("field", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("hsetnx", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hSetNx", c, p, a));
pub const M109: MethodEntry = MethodEntry::public_async(
    "hStrLen",
    &[
        Param::new("key", ParamType::String),
        Param::new("field", ParamType::String),
    ],
    |c, o, a| dispatch("hstrlen", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hStrLen", c, p, a));
