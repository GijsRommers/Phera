//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M70: MethodEntry =
    MethodEntry::public_async("get", &[Param::new("key", ParamType::String)], |c, o, a| {
        dispatch("get", c, o, a)
    })
    .with_preparation(|c, p, a| crate::arguments::prepare("Redis::get", c, p, a));
pub const M71: MethodEntry = MethodEntry::public_async(
    "getWithMeta",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("getwithmeta", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::getWithMeta", c, p, a));
pub const M72: MethodEntry =
    MethodEntry::public_async("getAuth", &[], |c, o, a| dispatch("getauth", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::getAuth", c, p, a));
pub const M73: MethodEntry = MethodEntry::public_async(
    "getBit",
    &[
        Param::new("key", ParamType::String),
        Param::new("idx", ParamType::Int),
    ],
    |c, o, a| dispatch("getbit", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::getBit", c, p, a));
pub const M74: MethodEntry = MethodEntry::public_async(
    "getEx",
    &[
        Param::new("key", ParamType::String),
        Param::new("options", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("getex", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::getEx", c, p, a));
pub const M75: MethodEntry =
    MethodEntry::public_async("getDBNum", &[], |c, o, a| dispatch("getdbnum", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::getDBNum", c, p, a));
pub const M76: MethodEntry = MethodEntry::public_async(
    "getDel",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("getdel", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::getDel", c, p, a));
pub const M77: MethodEntry =
    MethodEntry::public_async("getHost", &[], |c, o, a| dispatch("gethost", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::getHost", c, p, a));
pub const M78: MethodEntry = MethodEntry::public_async("getLastError", &[], |c, o, a| {
    dispatch("getlasterror", c, o, a)
})
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::getLastError", c, p, a));
pub const M79: MethodEntry =
    MethodEntry::public_async("getMode", &[], |c, o, a| dispatch("getmode", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::getMode", c, p, a));
