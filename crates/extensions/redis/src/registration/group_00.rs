//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.
use crate::dispatch::dispatch;
use phera_extension_api::{MethodEntry, Param, ParamType};
pub const M0: MethodEntry = MethodEntry::public_async(
    "__construct",
    &[Param::new("options", ParamType::Array)
        .nullable()
        .optional()],
    |c, o, a| dispatch("__construct", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::__construct", c, p, a));
pub const M1: MethodEntry =
    MethodEntry::public_async("__destruct", &[], |c, o, a| dispatch("__destruct", c, o, a))
        .with_preparation(|c, p, a| crate::arguments::prepare("Redis::__destruct", c, p, a));
pub const M2: MethodEntry = MethodEntry::public_async(
    "_compress",
    &[Param::new("value", ParamType::String)],
    |c, o, a| dispatch("_compress", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::_compress", c, p, a));
pub const M3: MethodEntry = MethodEntry::public_async(
    "_uncompress",
    &[Param::new("value", ParamType::String)],
    |c, o, a| dispatch("_uncompress", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::_uncompress", c, p, a));
pub const M4: MethodEntry = MethodEntry::public_async(
    "_prefix",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("_prefix", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::_prefix", c, p, a));
pub const M5: MethodEntry = MethodEntry::public_async(
    "_serialize",
    &[Param::new("value", ParamType::Mixed)],
    |c, o, a| dispatch("_serialize", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::_serialize", c, p, a));
pub const M6: MethodEntry = MethodEntry::public_async(
    "_unserialize",
    &[Param::new("value", ParamType::String)],
    |c, o, a| dispatch("_unserialize", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::_unserialize", c, p, a));
pub const M7: MethodEntry = MethodEntry::public_async(
    "_pack",
    &[Param::new("value", ParamType::Mixed)],
    |c, o, a| dispatch("_pack", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::_pack", c, p, a));
pub const M8: MethodEntry = MethodEntry::public_async(
    "_unpack",
    &[Param::new("value", ParamType::String)],
    |c, o, a| dispatch("_unpack", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::_unpack", c, p, a));
pub const M9: MethodEntry = MethodEntry::public_async(
    "acl",
    &[
        Param::new("subcmd", ParamType::String),
        Param::new("args", ParamType::String).optional().variadic(),
    ],
    |c, o, a| dispatch("acl", c, o, a),
)
.with_preparation(|c, p, a| crate::arguments::prepare("Redis::acl", c, p, a));
