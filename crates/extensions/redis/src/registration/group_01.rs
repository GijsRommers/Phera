//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M10: MethodEntry = MethodEntry::public_async(
    "append",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("append", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::append", c, p, a));
pub const M11: MethodEntry = MethodEntry::public_async(
    "auth",
    &[Param::new("credentials", ParamType::Mixed)],
    |c, o, a| dispatch("auth", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::auth", c, p, a));
pub const M12: MethodEntry =
    MethodEntry::public_async("bgSave", &[], |c, o, a| dispatch("bgsave", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::bgSave", c, p, a));
pub const M13: MethodEntry = MethodEntry::public_async("bgrewriteaof", &[], |c, o, a| {
    dispatch("bgrewriteaof", c, o, a)
})
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::bgrewriteaof", c, p, a));
pub const M14: MethodEntry = MethodEntry::public_async(
    "waitaof",
    &[
        Param::new("numlocal", ParamType::Int),
        Param::new("numreplicas", ParamType::Int),
        Param::new("timeout", ParamType::Int),
    ],
    |c, o, a| dispatch("waitaof", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::waitaof", c, p, a));
pub const M15: MethodEntry = MethodEntry::public_async(
    "bitcount",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::Int).optional(),
        Param::new("end", ParamType::Int).optional(),
        Param::new("bybit", ParamType::Bool).optional(),
    ],
    |c, o, a| dispatch("bitcount", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::bitcount", c, p, a));
pub const M16: MethodEntry = MethodEntry::public_async(
    "bitop",
    &[
        Param::new("operation", ParamType::String),
        Param::new("deskey", ParamType::String),
        Param::new("srckey", ParamType::String),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("bitop", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::bitop", c, p, a));
pub const M17: MethodEntry = MethodEntry::public_async(
    "bitpos",
    &[
        Param::new("key", ParamType::String),
        Param::new("bit", ParamType::Bool),
        Param::new("start", ParamType::Int).optional(),
        Param::new("end", ParamType::Int).optional(),
        Param::new("bybit", ParamType::Bool).optional(),
    ],
    |c, o, a| dispatch("bitpos", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::bitpos", c, p, a));
pub const M18: MethodEntry = MethodEntry::public_async(
    "blPop",
    &[
        Param::new("key_or_keys", ParamType::ArrayOrString),
        Param::new("timeout_or_key", ParamType::StringOrNumber),
        Param::new("extra_args", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("blpop", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::blPop", c, p, a));
pub const M19: MethodEntry = MethodEntry::public_async(
    "brPop",
    &[
        Param::new("key_or_keys", ParamType::ArrayOrString),
        Param::new("timeout_or_key", ParamType::StringOrNumber),
        Param::new("extra_args", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("brpop", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::brPop", c, p, a));
