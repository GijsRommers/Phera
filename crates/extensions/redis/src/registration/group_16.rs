//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M160: MethodEntry = MethodEntry::public_async(
    "ping",
    &[Param::new("message", ParamType::String)
        .nullable()
        .optional()],
    |c, o, a| dispatch("ping", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::ping", c, p, a));
pub const M161: MethodEntry =
    MethodEntry::public_async("pipeline", &[], |c, o, a| dispatch("pipeline", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::pipeline", c, p, a));
pub const M162: MethodEntry = MethodEntry::public_async(
    "popen",
    &[
        Param::new("host", ParamType::String),
        Param::new("port", ParamType::Int).optional(),
        Param::new("timeout", ParamType::Float).optional(),
        Param::new("persistent_id", ParamType::String)
            .nullable()
            .optional(),
        Param::new("retry_interval", ParamType::Int).optional(),
        Param::new("read_timeout", ParamType::Float).optional(),
        Param::new("context", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("popen", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::popen", c, p, a));
pub const M163: MethodEntry = MethodEntry::public_async(
    "psetex",
    &[
        Param::new("key", ParamType::String),
        Param::new("expire", ParamType::Int),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("psetex", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::psetex", c, p, a));
pub const M164: MethodEntry = MethodEntry::public_async(
    "psubscribe",
    &[
        Param::new("patterns", ParamType::Array),
        Param::new("cb", ParamType::Callable),
    ],
    |c, o, a| dispatch("psubscribe", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::psubscribe", c, p, a));
pub const M165: MethodEntry = MethodEntry::public_async(
    "pttl",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("pttl", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::pttl", c, p, a));
pub const M166: MethodEntry = MethodEntry::public_async(
    "publish",
    &[
        Param::new("channel", ParamType::String),
        Param::new("message", ParamType::String),
    ],
    |c, o, a| dispatch("publish", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::publish", c, p, a));
pub const M167: MethodEntry = MethodEntry::public_async(
    "pubsub",
    &[
        Param::new("command", ParamType::String),
        Param::new("arg", ParamType::Mixed).optional(),
    ],
    |c, o, a| dispatch("pubsub", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::pubsub", c, p, a));
pub const M168: MethodEntry = MethodEntry::public_async(
    "punsubscribe",
    &[Param::new("patterns", ParamType::Array)],
    |c, o, a| dispatch("punsubscribe", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::punsubscribe", c, p, a));
pub const M169: MethodEntry = MethodEntry::public_async(
    "rPop",
    &[
        Param::new("key", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("rpop", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::rPop", c, p, a));
