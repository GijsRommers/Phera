//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M80: MethodEntry = MethodEntry::public_async(
    "getOption",
    &[Param::new("option", ParamType::Int)],
    |c, o, a| dispatch("getoption", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::getOption", c, p, a));
pub const M81: MethodEntry = MethodEntry::public_async("getPersistentID", &[], |c, o, a| {
    dispatch("getpersistentid", c, o, a)
})
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::getPersistentID", c, p, a));
pub const M82: MethodEntry =
    MethodEntry::public_async("getPort", &[], |c, o, a| dispatch("getport", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::getPort", c, p, a));
pub const M83: MethodEntry =
    MethodEntry::public_async("serverName", &[], |c, o, a| dispatch("servername", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::serverName", c, p, a));
pub const M84: MethodEntry = MethodEntry::public_async("serverVersion", &[], |c, o, a| {
    dispatch("serverversion", c, o, a)
})
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::serverVersion", c, p, a));
pub const M85: MethodEntry = MethodEntry::public_async(
    "getRange",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::Int),
        Param::new("end", ParamType::Int),
    ],
    |c, o, a| dispatch("getrange", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::getRange", c, p, a));
pub const M86: MethodEntry = MethodEntry::public_async(
    "lcs",
    &[
        Param::new("key1", ParamType::String),
        Param::new("key2", ParamType::String),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("lcs", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::lcs", c, p, a));
pub const M87: MethodEntry = MethodEntry::public_async("getReadTimeout", &[], |c, o, a| {
    dispatch("getreadtimeout", c, o, a)
})
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::getReadTimeout", c, p, a));
pub const M88: MethodEntry = MethodEntry::public_async(
    "getset",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("getset", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::getset", c, p, a));
pub const M89: MethodEntry =
    MethodEntry::public_async("getTimeout", &[], |c, o, a| dispatch("gettimeout", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::getTimeout", c, p, a));
