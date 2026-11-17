//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M190: MethodEntry = MethodEntry::public_async(
    "sUnion",
    &[
        Param::new("key", ParamType::String),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("sunion", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sUnion", c, p, a));
pub const M191: MethodEntry = MethodEntry::public_async(
    "sUnionStore",
    &[
        Param::new("dst", ParamType::String),
        Param::new("key", ParamType::String),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("sunionstore", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sUnionStore", c, p, a));
pub const M192: MethodEntry =
    MethodEntry::public_async("save", &[], |c, o, a| dispatch("save", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::save", c, p, a));
pub const M193: MethodEntry = MethodEntry::public_async(
    "scan",
    &[
        Param::new("iterator", ParamType::IntOrString)
            .nullable()
            .by_ref(),
        Param::new("pattern", ParamType::String)
            .nullable()
            .optional(),
        Param::new("count", ParamType::Int).optional(),
        Param::new("type", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("scan", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::scan", c, p, a));
pub const M194: MethodEntry = MethodEntry::public_async(
    "scard",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("scard", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::scard", c, p, a));
pub const M195: MethodEntry = MethodEntry::public_async(
    "script",
    &[
        Param::new("command", ParamType::String),
        Param::new("args", ParamType::Mixed).optional().variadic(),
    ],
    |c, o, a| dispatch("script", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::script", c, p, a));
pub const M196: MethodEntry =
    MethodEntry::public_async("select", &[Param::new("db", ParamType::Int)], |c, o, a| {
        dispatch("select", c, o, a)
    })
    .with_preparation(|c, p, a| crate::arguments::prepare("Redis::select", c, p, a));
pub const M197: MethodEntry = MethodEntry::public_async(
    "set",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
        Param::new("options", ParamType::Mixed).optional(),
    ],
    |c, o, a| dispatch("set", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::set", c, p, a));
pub const M198: MethodEntry = MethodEntry::public_async(
    "setBit",
    &[
        Param::new("key", ParamType::String),
        Param::new("idx", ParamType::Int),
        Param::new("value", ParamType::Bool),
    ],
    |c, o, a| dispatch("setbit", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::setBit", c, p, a));
pub const M199: MethodEntry = MethodEntry::public_async(
    "setRange",
    &[
        Param::new("key", ParamType::String),
        Param::new("index", ParamType::Int),
        Param::new("value", ParamType::String),
    ],
    |c, o, a| dispatch("setrange", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::setRange", c, p, a));
