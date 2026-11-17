//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M270: MethodEntry = MethodEntry::public_async(
    "zRem",
    &[
        Param::new("key", ParamType::Mixed),
        Param::new("member", ParamType::Mixed),
        Param::new("other_members", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("zrem", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zRem", c, p, a));
pub const M271: MethodEntry = MethodEntry::public_async(
    "zRemRangeByLex",
    &[
        Param::new("key", ParamType::String),
        Param::new("min", ParamType::String),
        Param::new("max", ParamType::String),
    ],
    |c, o, a| dispatch("zremrangebylex", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zRemRangeByLex", c, p, a));
pub const M272: MethodEntry = MethodEntry::public_async(
    "zRemRangeByRank",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::Int),
        Param::new("end", ParamType::Int),
    ],
    |c, o, a| dispatch("zremrangebyrank", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zRemRangeByRank", c, p, a));
pub const M273: MethodEntry = MethodEntry::public_async(
    "zRemRangeByScore",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::String),
        Param::new("end", ParamType::String),
    ],
    |c, o, a| dispatch("zremrangebyscore", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zRemRangeByScore", c, p, a));
pub const M274: MethodEntry = MethodEntry::public_async(
    "zRevRange",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::Int),
        Param::new("end", ParamType::Int),
        Param::new("scores", ParamType::Mixed).optional(),
    ],
    |c, o, a| dispatch("zrevrange", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zRevRange", c, p, a));
pub const M275: MethodEntry = MethodEntry::public_async(
    "zRevRangeByLex",
    &[
        Param::new("key", ParamType::String),
        Param::new("max", ParamType::String),
        Param::new("min", ParamType::String),
        Param::new("offset", ParamType::Int).optional(),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("zrevrangebylex", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zRevRangeByLex", c, p, a));
pub const M276: MethodEntry = MethodEntry::public_async(
    "zRevRangeByScore",
    &[
        Param::new("key", ParamType::String),
        Param::new("max", ParamType::String),
        Param::new("min", ParamType::String),
        Param::new("options", ParamType::ArrayOrBool).optional(),
    ],
    |c, o, a| dispatch("zrevrangebyscore", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zRevRangeByScore", c, p, a));
pub const M277: MethodEntry = MethodEntry::public_async(
    "zRevRank",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
    ],
    |c, o, a| dispatch("zrevrank", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zRevRank", c, p, a));
pub const M278: MethodEntry = MethodEntry::public_async(
    "zScore",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
    ],
    |c, o, a| dispatch("zscore", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zScore", c, p, a));
pub const M279: MethodEntry = MethodEntry::public_async(
    "zdiff",
    &[
        Param::new("keys", ParamType::Array),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("zdiff", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zdiff", c, p, a));
