//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M110: MethodEntry = MethodEntry::public_async(
    "hVals",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("hvals", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hVals", c, p, a));
pub const M111: MethodEntry = MethodEntry::public_async(
    "hexpire",
    &[
        Param::new("key", ParamType::String),
        Param::new("ttl", ParamType::Int),
        Param::new("fields", ParamType::Array),
        Param::new("mode", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("hexpire", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hexpire", c, p, a));
pub const M112: MethodEntry = MethodEntry::public_async(
    "hpexpire",
    &[
        Param::new("key", ParamType::String),
        Param::new("ttl", ParamType::Int),
        Param::new("fields", ParamType::Array),
        Param::new("mode", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("hpexpire", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hpexpire", c, p, a));
pub const M113: MethodEntry = MethodEntry::public_async(
    "hexpireat",
    &[
        Param::new("key", ParamType::String),
        Param::new("time", ParamType::Int),
        Param::new("fields", ParamType::Array),
        Param::new("mode", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("hexpireat", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hexpireat", c, p, a));
pub const M114: MethodEntry = MethodEntry::public_async(
    "hpexpireat",
    &[
        Param::new("key", ParamType::String),
        Param::new("mstime", ParamType::Int),
        Param::new("fields", ParamType::Array),
        Param::new("mode", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("hpexpireat", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hpexpireat", c, p, a));
pub const M115: MethodEntry = MethodEntry::public_async(
    "httl",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
    ],
    |c, o, a| dispatch("httl", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::httl", c, p, a));
pub const M116: MethodEntry = MethodEntry::public_async(
    "hpttl",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
    ],
    |c, o, a| dispatch("hpttl", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hpttl", c, p, a));
pub const M117: MethodEntry = MethodEntry::public_async(
    "hexpiretime",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
    ],
    |c, o, a| dispatch("hexpiretime", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hexpiretime", c, p, a));
pub const M118: MethodEntry = MethodEntry::public_async(
    "hpexpiretime",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
    ],
    |c, o, a| dispatch("hpexpiretime", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hpexpiretime", c, p, a));
pub const M119: MethodEntry = MethodEntry::public_async(
    "hpersist",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
    ],
    |c, o, a| dispatch("hpersist", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hpersist", c, p, a));
