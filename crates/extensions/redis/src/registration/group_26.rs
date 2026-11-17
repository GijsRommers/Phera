//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M260: MethodEntry = MethodEntry::public_async(
    "zLexCount",
    &[
        Param::new("key", ParamType::String),
        Param::new("min", ParamType::String),
        Param::new("max", ParamType::String),
    ],
    |c, o, a| dispatch("zlexcount", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zLexCount", c, p, a));
pub const M261: MethodEntry = MethodEntry::public_async(
    "zMscore",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
        Param::new("other_members", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("zmscore", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zMscore", c, p, a));
pub const M262: MethodEntry = MethodEntry::public_async(
    "zPopMax",
    &[
        Param::new("key", ParamType::String),
        Param::new("count", ParamType::Int).nullable().optional(),
    ],
    |c, o, a| dispatch("zpopmax", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zPopMax", c, p, a));
pub const M263: MethodEntry = MethodEntry::public_async(
    "zPopMin",
    &[
        Param::new("key", ParamType::String),
        Param::new("count", ParamType::Int).nullable().optional(),
    ],
    |c, o, a| dispatch("zpopmin", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zPopMin", c, p, a));
pub const M264: MethodEntry = MethodEntry::public_async(
    "zRange",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::IntOrString),
        Param::new("end", ParamType::IntOrString),
        Param::new("options", ParamType::ArrayOrBool)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("zrange", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zRange", c, p, a));
pub const M265: MethodEntry = MethodEntry::public_async(
    "zRangeByLex",
    &[
        Param::new("key", ParamType::String),
        Param::new("min", ParamType::String),
        Param::new("max", ParamType::String),
        Param::new("offset", ParamType::Int).optional(),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("zrangebylex", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zRangeByLex", c, p, a));
pub const M266: MethodEntry = MethodEntry::public_async(
    "zRangeByScore",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::String),
        Param::new("end", ParamType::String),
        Param::new("options", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("zrangebyscore", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zRangeByScore", c, p, a));
pub const M267: MethodEntry = MethodEntry::public_async(
    "zrangestore",
    &[
        Param::new("dstkey", ParamType::String),
        Param::new("srckey", ParamType::String),
        Param::new("start", ParamType::String),
        Param::new("end", ParamType::String),
        Param::new("options", ParamType::ArrayOrBool)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("zrangestore", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zrangestore", c, p, a));
pub const M268: MethodEntry = MethodEntry::public_async(
    "zRandMember",
    &[
        Param::new("key", ParamType::String),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("zrandmember", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zRandMember", c, p, a));
pub const M269: MethodEntry = MethodEntry::public_async(
    "zRank",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
    ],
    |c, o, a| dispatch("zrank", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zRank", c, p, a));
