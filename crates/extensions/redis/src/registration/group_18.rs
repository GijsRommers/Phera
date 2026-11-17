//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M180: MethodEntry = MethodEntry::public_async(
    "sDiff",
    &[
        Param::new("key", ParamType::String),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("sdiff", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sDiff", c, p, a));
pub const M181: MethodEntry = MethodEntry::public_async(
    "sDiffStore",
    &[
        Param::new("dst", ParamType::String),
        Param::new("key", ParamType::String),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("sdiffstore", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sDiffStore", c, p, a));
pub const M182: MethodEntry = MethodEntry::public_async(
    "sInter",
    &[
        Param::new("key", ParamType::ArrayOrString),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("sinter", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sInter", c, p, a));
pub const M183: MethodEntry = MethodEntry::public_async(
    "sintercard",
    &[
        Param::new("keys", ParamType::Array),
        Param::new("limit", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("sintercard", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sintercard", c, p, a));
pub const M184: MethodEntry = MethodEntry::public_async(
    "sInterStore",
    &[
        Param::new("key", ParamType::ArrayOrString),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("sinterstore", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sInterStore", c, p, a));
pub const M185: MethodEntry = MethodEntry::public_async(
    "sMembers",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("smembers", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sMembers", c, p, a));
pub const M186: MethodEntry = MethodEntry::public_async(
    "sMisMember",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::String),
        Param::new("other_members", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("smismember", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sMisMember", c, p, a));
pub const M187: MethodEntry = MethodEntry::public_async(
    "sMove",
    &[
        Param::new("src", ParamType::String),
        Param::new("dst", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("smove", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sMove", c, p, a));
pub const M188: MethodEntry = MethodEntry::public_async(
    "sPop",
    &[
        Param::new("key", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("spop", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sPop", c, p, a));
pub const M189: MethodEntry = MethodEntry::public_async(
    "sRandMember",
    &[
        Param::new("key", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("srandmember", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sRandMember", c, p, a));
