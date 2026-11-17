//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M150: MethodEntry = MethodEntry::public_async(
    "multi",
    &[Param::new("value", ParamType::Int).optional()],
    |c, o, a| dispatch("multi", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::multi", c, p, a));
pub const M151: MethodEntry = MethodEntry::public_async(
    "object",
    &[
        Param::new("subcommand", ParamType::String),
        Param::new("key", ParamType::String),
    ],
    |c, o, a| dispatch("object", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::object", c, p, a));
pub const M152: MethodEntry = MethodEntry::public_async(
    "open",
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
    |c, o, a| dispatch("open", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::open", c, p, a));
pub const M153: MethodEntry = MethodEntry::public_async(
    "pconnect",
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
    |c, o, a| dispatch("pconnect", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::pconnect", c, p, a));
pub const M154: MethodEntry = MethodEntry::public_async(
    "persist",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("persist", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::persist", c, p, a));
pub const M155: MethodEntry = MethodEntry::public_async(
    "pexpire",
    &[
        Param::new("key", ParamType::String),
        Param::new("timeout", ParamType::Int),
        Param::new("mode", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("pexpire", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::pexpire", c, p, a));
pub const M156: MethodEntry = MethodEntry::public_async(
    "pexpireAt",
    &[
        Param::new("key", ParamType::String),
        Param::new("timestamp", ParamType::Int),
        Param::new("mode", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("pexpireat", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::pexpireAt", c, p, a));
pub const M157: MethodEntry = MethodEntry::public_async(
    "pfadd",
    &[
        Param::new("key", ParamType::String),
        Param::new("elements", ParamType::Array),
    ],
    |c, o, a| dispatch("pfadd", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::pfadd", c, p, a));
pub const M158: MethodEntry = MethodEntry::public_async(
    "pfcount",
    &[Param::new("key_or_keys", ParamType::ArrayOrString)],
    |c, o, a| dispatch("pfcount", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::pfcount", c, p, a));
pub const M159: MethodEntry = MethodEntry::public_async(
    "pfmerge",
    &[
        Param::new("dst", ParamType::String),
        Param::new("srckeys", ParamType::Array),
    ],
    |c, o, a| dispatch("pfmerge", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::pfmerge", c, p, a));
