//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M210: MethodEntry = MethodEntry::public_async(
    "sortAsc",
    &[
        Param::new("key", ParamType::String),
        Param::new("pattern", ParamType::String)
            .nullable()
            .optional(),
        Param::new("get", ParamType::Mixed).optional(),
        Param::new("offset", ParamType::Int).optional(),
        Param::new("count", ParamType::Int).optional(),
        Param::new("store", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("sortasc", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sortAsc", c, p, a));
pub const M211: MethodEntry = MethodEntry::public_async(
    "sortAscAlpha",
    &[
        Param::new("key", ParamType::String),
        Param::new("pattern", ParamType::String)
            .nullable()
            .optional(),
        Param::new("get", ParamType::Mixed).optional(),
        Param::new("offset", ParamType::Int).optional(),
        Param::new("count", ParamType::Int).optional(),
        Param::new("store", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("sortascalpha", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sortAscAlpha", c, p, a));
pub const M212: MethodEntry = MethodEntry::public_async(
    "sortDesc",
    &[
        Param::new("key", ParamType::String),
        Param::new("pattern", ParamType::String)
            .nullable()
            .optional(),
        Param::new("get", ParamType::Mixed).optional(),
        Param::new("offset", ParamType::Int).optional(),
        Param::new("count", ParamType::Int).optional(),
        Param::new("store", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("sortdesc", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sortDesc", c, p, a));
pub const M213: MethodEntry = MethodEntry::public_async(
    "sortDescAlpha",
    &[
        Param::new("key", ParamType::String),
        Param::new("pattern", ParamType::String)
            .nullable()
            .optional(),
        Param::new("get", ParamType::Mixed).optional(),
        Param::new("offset", ParamType::Int).optional(),
        Param::new("count", ParamType::Int).optional(),
        Param::new("store", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("sortdescalpha", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sortDescAlpha", c, p, a));
pub const M214: MethodEntry = MethodEntry::public_async(
    "srem",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
        Param::new("other_values", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("srem", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::srem", c, p, a));
pub const M215: MethodEntry = MethodEntry::public_async(
    "sscan",
    &[
        Param::new("key", ParamType::String),
        Param::new("iterator", ParamType::IntOrString)
            .nullable()
            .by_ref(),
        Param::new("pattern", ParamType::String)
            .nullable()
            .optional(),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("sscan", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sscan", c, p, a));
pub const M216: MethodEntry = MethodEntry::public_async(
    "ssubscribe",
    &[
        Param::new("channels", ParamType::Array),
        Param::new("cb", ParamType::Callable),
    ],
    |c, o, a| dispatch("ssubscribe", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::ssubscribe", c, p, a));
pub const M217: MethodEntry = MethodEntry::public_async(
    "strlen",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("strlen", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::strlen", c, p, a));
pub const M218: MethodEntry = MethodEntry::public_async(
    "subscribe",
    &[
        Param::new("channels", ParamType::Array),
        Param::new("cb", ParamType::Callable),
    ],
    |c, o, a| dispatch("subscribe", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::subscribe", c, p, a));
pub const M219: MethodEntry = MethodEntry::public_async(
    "sunsubscribe",
    &[Param::new("channels", ParamType::Array)],
    |c, o, a| dispatch("sunsubscribe", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sunsubscribe", c, p, a));
