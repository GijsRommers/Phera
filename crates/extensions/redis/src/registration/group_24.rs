//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M240: MethodEntry = MethodEntry::public_async(
    "xreadgroup",
    &[
        Param::new("group", ParamType::String),
        Param::new("consumer", ParamType::String),
        Param::new("streams", ParamType::Array),
        Param::new("count", ParamType::Int).optional(),
        Param::new("block", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("xreadgroup", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::xreadgroup", c, p, a));
pub const M241: MethodEntry = MethodEntry::public_async(
    "xrevrange",
    &[
        Param::new("key", ParamType::String),
        Param::new("end", ParamType::String),
        Param::new("start", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("xrevrange", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::xrevrange", c, p, a));
pub const M242: MethodEntry = MethodEntry::public_async(
    "vadd",
    &[
        Param::new("key", ParamType::String),
        Param::new("values", ParamType::Array),
        Param::new("element", ParamType::Mixed),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("vadd", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::vadd", c, p, a));
pub const M243: MethodEntry = MethodEntry::public_async(
    "vsim",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("vsim", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::vsim", c, p, a));
pub const M244: MethodEntry = MethodEntry::public_async(
    "vcard",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("vcard", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::vcard", c, p, a));
pub const M245: MethodEntry = MethodEntry::public_async(
    "vdim",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("vdim", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::vdim", c, p, a));
pub const M246: MethodEntry = MethodEntry::public_async(
    "vinfo",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("vinfo", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::vinfo", c, p, a));
pub const M247: MethodEntry = MethodEntry::public_async(
    "vismember",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
    ],
    |c, o, a| dispatch("vismember", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::vismember", c, p, a));
pub const M248: MethodEntry = MethodEntry::public_async(
    "vemb",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
        Param::new("raw", ParamType::Bool).optional(),
    ],
    |c, o, a| dispatch("vemb", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::vemb", c, p, a));
pub const M249: MethodEntry = MethodEntry::public_async(
    "vrandmember",
    &[
        Param::new("key", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("vrandmember", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::vrandmember", c, p, a));
