//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M280: MethodEntry = MethodEntry::public_async(
    "zdiffstore",
    &[
        Param::new("dst", ParamType::String),
        Param::new("keys", ParamType::Array),
    ],
    |c, o, a| dispatch("zdiffstore", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zdiffstore", c, p, a));
pub const M281: MethodEntry = MethodEntry::public_async(
    "zinter",
    &[
        Param::new("keys", ParamType::Array),
        Param::new("weights", ParamType::Array)
            .nullable()
            .optional(),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("zinter", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zinter", c, p, a));
pub const M282: MethodEntry = MethodEntry::public_async(
    "zintercard",
    &[
        Param::new("keys", ParamType::Array),
        Param::new("limit", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("zintercard", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zintercard", c, p, a));
pub const M283: MethodEntry = MethodEntry::public_async(
    "zinterstore",
    &[
        Param::new("dst", ParamType::String),
        Param::new("keys", ParamType::Array),
        Param::new("weights", ParamType::Array)
            .nullable()
            .optional(),
        Param::new("aggregate", ParamType::String)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("zinterstore", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zinterstore", c, p, a));
pub const M284: MethodEntry = MethodEntry::public_async(
    "zscan",
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
    |c, o, a| dispatch("zscan", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zscan", c, p, a));
pub const M285: MethodEntry = MethodEntry::public_async(
    "zunion",
    &[
        Param::new("keys", ParamType::Array),
        Param::new("weights", ParamType::Array)
            .nullable()
            .optional(),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("zunion", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zunion", c, p, a));
pub const M286: MethodEntry = MethodEntry::public_async(
    "zunionstore",
    &[
        Param::new("dst", ParamType::String),
        Param::new("keys", ParamType::Array),
        Param::new("weights", ParamType::Array)
            .nullable()
            .optional(),
        Param::new("aggregate", ParamType::String)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("zunionstore", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::zunionstore", c, p, a));
