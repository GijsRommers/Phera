//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M170: MethodEntry =
    MethodEntry::public_async("randomKey", &[], |c, o, a| dispatch("randomkey", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::randomKey", c, p, a));
pub const M171: MethodEntry = MethodEntry::public_async(
    "rawcommand",
    &[
        Param::new("command", ParamType::String),
        Param::new("args", ParamType::Mixed).optional().variadic(),
    ],
    |c, o, a| dispatch("rawcommand", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::rawcommand", c, p, a));
pub const M172: MethodEntry = MethodEntry::public_async(
    "rename",
    &[
        Param::new("old_name", ParamType::String),
        Param::new("new_name", ParamType::String),
    ],
    |c, o, a| dispatch("rename", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::rename", c, p, a));
pub const M173: MethodEntry = MethodEntry::public_async(
    "renameNx",
    &[
        Param::new("key_src", ParamType::String),
        Param::new("key_dst", ParamType::String),
    ],
    |c, o, a| dispatch("renamenx", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::renameNx", c, p, a));
pub const M174: MethodEntry =
    MethodEntry::public_async("reset", &[], |c, o, a| dispatch("reset", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::reset", c, p, a));
pub const M175: MethodEntry = MethodEntry::public_async(
    "restore",
    &[
        Param::new("key", ParamType::String),
        Param::new("ttl", ParamType::Int),
        Param::new("value", ParamType::String),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("restore", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::restore", c, p, a));
pub const M176: MethodEntry =
    MethodEntry::public_async("role", &[], |c, o, a| dispatch("role", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::role", c, p, a));
pub const M177: MethodEntry = MethodEntry::public_async(
    "rpoplpush",
    &[
        Param::new("srckey", ParamType::String),
        Param::new("dstkey", ParamType::String),
    ],
    |c, o, a| dispatch("rpoplpush", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::rpoplpush", c, p, a));
pub const M178: MethodEntry = MethodEntry::public_async(
    "sAdd",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
        Param::new("other_values", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("sadd", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sAdd", c, p, a));
pub const M179: MethodEntry = MethodEntry::public_async(
    "sAddArray",
    &[
        Param::new("key", ParamType::String),
        Param::new("values", ParamType::Array),
    ],
    |c, o, a| dispatch("saddarray", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::sAddArray", c, p, a));
