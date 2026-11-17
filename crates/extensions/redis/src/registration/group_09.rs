//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M90: MethodEntry = MethodEntry::public_async("getTransferredBytes", &[], |c, o, a| {
    dispatch("gettransferredbytes", c, o, a)
})
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::getTransferredBytes", c, p, a));
pub const M91: MethodEntry = MethodEntry::public_async("clearTransferredBytes", &[], |c, o, a| {
    dispatch("cleartransferredbytes", c, o, a)
})
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::clearTransferredBytes", c, p, a));
pub const M92: MethodEntry = MethodEntry::public_async(
    "hDel",
    &[
        Param::new("key", ParamType::String),
        Param::new("field", ParamType::String),
        Param::new("other_fields", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("hdel", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hDel", c, p, a));
pub const M93: MethodEntry = MethodEntry::public_async(
    "hExists",
    &[
        Param::new("key", ParamType::String),
        Param::new("field", ParamType::String),
    ],
    |c, o, a| dispatch("hexists", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hExists", c, p, a));
pub const M94: MethodEntry = MethodEntry::public_async(
    "hGet",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::String),
    ],
    |c, o, a| dispatch("hget", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hGet", c, p, a));
pub const M95: MethodEntry = MethodEntry::public_async(
    "hGetAll",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("hgetall", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hGetAll", c, p, a));
pub const M96: MethodEntry = MethodEntry::public_async(
    "hGetWithMeta",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::String),
    ],
    |c, o, a| dispatch("hgetwithmeta", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hGetWithMeta", c, p, a));
pub const M97: MethodEntry = MethodEntry::public_async(
    "hIncrBy",
    &[
        Param::new("key", ParamType::String),
        Param::new("field", ParamType::String),
        Param::new("value", ParamType::Int),
    ],
    |c, o, a| dispatch("hincrby", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hIncrBy", c, p, a));
pub const M98: MethodEntry = MethodEntry::public_async(
    "hIncrByFloat",
    &[
        Param::new("key", ParamType::String),
        Param::new("field", ParamType::String),
        Param::new("value", ParamType::Float),
    ],
    |c, o, a| dispatch("hincrbyfloat", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hIncrByFloat", c, p, a));
pub const M99: MethodEntry = MethodEntry::public_async(
    "hKeys",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("hkeys", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::hKeys", c, p, a));
