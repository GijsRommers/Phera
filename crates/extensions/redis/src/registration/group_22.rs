//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M220: MethodEntry = MethodEntry::public_async(
    "swapdb",
    &[
        Param::new("src", ParamType::Int),
        Param::new("dst", ParamType::Int),
    ],
    |c, o, a| dispatch("swapdb", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::swapdb", c, p, a));
pub const M221: MethodEntry =
    MethodEntry::public_async("time", &[], |c, o, a| dispatch("time", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::time", c, p, a));
pub const M222: MethodEntry =
    MethodEntry::public_async("ttl", &[Param::new("key", ParamType::String)], |c, o, a| {
        dispatch("ttl", c, o, a)
    })
    .with_preparation(|c, p, a| crate::arguments::prepare("Redis::ttl", c, p, a));
pub const M223: MethodEntry = MethodEntry::public_async(
    "type",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("type", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::type", c, p, a));
pub const M224: MethodEntry = MethodEntry::public_async(
    "unlink",
    &[
        Param::new("key", ParamType::ArrayOrString),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("unlink", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::unlink", c, p, a));
pub const M225: MethodEntry = MethodEntry::public_async(
    "unsubscribe",
    &[Param::new("channels", ParamType::Array)],
    |c, o, a| dispatch("unsubscribe", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::unsubscribe", c, p, a));
pub const M226: MethodEntry =
    MethodEntry::public_async("unwatch", &[], |c, o, a| dispatch("unwatch", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::unwatch", c, p, a));
pub const M227: MethodEntry = MethodEntry::public_async(
    "watch",
    &[
        Param::new("key", ParamType::ArrayOrString),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("watch", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::watch", c, p, a));
pub const M228: MethodEntry = MethodEntry::public_async(
    "wait",
    &[
        Param::new("numreplicas", ParamType::Int),
        Param::new("timeout", ParamType::Int),
    ],
    |c, o, a| dispatch("wait", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::wait", c, p, a));
pub const M229: MethodEntry = MethodEntry::public_async(
    "xack",
    &[
        Param::new("key", ParamType::String),
        Param::new("group", ParamType::String),
        Param::new("ids", ParamType::Array),
    ],
    |c, o, a| dispatch("xack", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::xack", c, p, a));
