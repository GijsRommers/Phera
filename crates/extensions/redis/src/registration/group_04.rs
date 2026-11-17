//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M40: MethodEntry = MethodEntry::public_async(
    "delete",
    &[
        Param::new("key", ParamType::ArrayOrString),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("delete", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::delete", c, p, a));
pub const M41: MethodEntry =
    MethodEntry::public_async("discard", &[], |c, o, a| dispatch("discard", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::discard", c, p, a));
pub const M42: MethodEntry = MethodEntry::public_async(
    "dump",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("dump", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::dump", c, p, a));
pub const M43: MethodEntry = MethodEntry::public_async(
    "echo",
    &[Param::new("str", ParamType::String)],
    |c, o, a| dispatch("echo", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::echo", c, p, a));
pub const M44: MethodEntry = MethodEntry::public_async(
    "eval",
    &[
        Param::new("script", ParamType::String),
        Param::new("args", ParamType::Array).optional(),
        Param::new("num_keys", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("eval", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::eval", c, p, a));
pub const M45: MethodEntry = MethodEntry::public_async(
    "eval_ro",
    &[
        Param::new("script_sha", ParamType::String),
        Param::new("args", ParamType::Array).optional(),
        Param::new("num_keys", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("eval_ro", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::eval_ro", c, p, a));
pub const M46: MethodEntry = MethodEntry::public_async(
    "evalsha",
    &[
        Param::new("sha1", ParamType::String),
        Param::new("args", ParamType::Array).optional(),
        Param::new("num_keys", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("evalsha", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::evalsha", c, p, a));
pub const M47: MethodEntry = MethodEntry::public_async(
    "evalsha_ro",
    &[
        Param::new("sha1", ParamType::String),
        Param::new("args", ParamType::Array).optional(),
        Param::new("num_keys", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("evalsha_ro", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::evalsha_ro", c, p, a));
pub const M48: MethodEntry =
    MethodEntry::public_async("exec", &[], |c, o, a| dispatch("exec", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::exec", c, p, a));
pub const M49: MethodEntry = MethodEntry::public_async(
    "exists",
    &[
        Param::new("key", ParamType::Mixed),
        Param::new("other_keys", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("exists", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::exists", c, p, a));
