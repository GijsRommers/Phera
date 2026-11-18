//! Streams: the PHP signatures, built in `crate::commands::streams`.

use phera_extension_api::{MethodEntry, Param, ParamType};

use crate::arguments;
use crate::dispatch::dispatch;

/// `$redis->xack(string $key, string $group, array $ids)`
pub const XACK: MethodEntry = MethodEntry::public_async(
    "xack",
    &[
        Param::new("key", ParamType::String),
        Param::new("group", ParamType::String),
        Param::new("ids", ParamType::Array),
    ],
    |c, o, a| dispatch("xack", c, o, a),
);

/// `$redis->xadd(string $key, string $id, array $values, int $maxlen = 0, bool $approx = false, bool $nomkstream = false)`
pub const XADD: MethodEntry = MethodEntry::public_async(
    "xadd",
    &[
        Param::new("key", ParamType::String),
        Param::new("id", ParamType::String),
        Param::new("values", ParamType::Array),
        Param::new("maxlen", ParamType::Int).optional(),
        Param::new("approx", ParamType::Bool).optional(),
        Param::new("nomkstream", ParamType::Bool).optional(),
    ],
    |c, o, a| dispatch("xadd", c, o, a),
);

/// `$redis->xautoclaim(string $key, string $group, string $consumer, int $min_idle, string $start, int $count = -1, bool $justid = false)`
pub const XAUTOCLAIM: MethodEntry = MethodEntry::public_async(
    "xautoclaim",
    &[
        Param::new("key", ParamType::String),
        Param::new("group", ParamType::String),
        Param::new("consumer", ParamType::String),
        Param::new("min_idle", ParamType::Int),
        Param::new("start", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
        Param::new("justid", ParamType::Bool).optional(),
    ],
    |c, o, a| dispatch("xautoclaim", c, o, a),
);

/// `$redis->xclaim(string $key, string $group, string $consumer, int $min_idle, array $ids, array $options)`
pub const XCLAIM: MethodEntry = MethodEntry::public_async(
    "xclaim",
    &[
        Param::new("key", ParamType::String),
        Param::new("group", ParamType::String),
        Param::new("consumer", ParamType::String),
        Param::new("min_idle", ParamType::Int),
        Param::new("ids", ParamType::Array),
        Param::new("options", ParamType::Array),
    ],
    |c, o, a| dispatch("xclaim", c, o, a),
)
.with_preparation(|c, _, a| arguments::parse_as("Redis::xclaim", XCLAIM_PARSED, c, a));
/// How phpredis parses `xclaim()`, which differs from its declaration.
const XCLAIM_PARSED: &[Param] = &[
    Param::new("key", ParamType::String),
    Param::new("group", ParamType::String),
    Param::new("consumer", ParamType::String),
    Param::new("min_idle", ParamType::Int),
    Param::new("ids", ParamType::Array),
    Param::new("options", ParamType::Array).optional(),
];

/// `$redis->xdel(string $key, array $ids)`
pub const XDEL: MethodEntry = MethodEntry::public_async(
    "xdel",
    &[
        Param::new("key", ParamType::String),
        Param::new("ids", ParamType::Array),
    ],
    |c, o, a| dispatch("xdel", c, o, a),
);

/// `$redis->xgroup(string $operation, ?string $key = null, ?string $group = null, ?string $id_or_consumer = null, bool $mkstream = false, int $entries_read = -2)`
pub const XGROUP: MethodEntry = MethodEntry::public_async(
    "xgroup",
    &[
        Param::new("operation", ParamType::String),
        Param::new("key", ParamType::String).nullable().optional(),
        Param::new("group", ParamType::String).nullable().optional(),
        Param::new("id_or_consumer", ParamType::String)
            .nullable()
            .optional(),
        Param::new("mkstream", ParamType::Bool).optional(),
        Param::new("entries_read", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("xgroup", c, o, a),
);

/// `$redis->xinfo(string $operation, ?string $arg1 = null, ?string $arg2 = null, int $count = -1)`
pub const XINFO: MethodEntry = MethodEntry::public_async(
    "xinfo",
    &[
        Param::new("operation", ParamType::String),
        Param::new("arg1", ParamType::String).nullable().optional(),
        Param::new("arg2", ParamType::String).nullable().optional(),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("xinfo", c, o, a),
);

/// `$redis->xlen(string $key)`
pub const XLEN: MethodEntry = MethodEntry::public_async(
    "xlen",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("xlen", c, o, a),
);

/// `$redis->xpending(string $key, string $group, ?string $start = null, ?string $end = null, int $count = -1, ?string $consumer = null)`
pub const XPENDING: MethodEntry = MethodEntry::public_async(
    "xpending",
    &[
        Param::new("key", ParamType::String),
        Param::new("group", ParamType::String),
        Param::new("start", ParamType::String).nullable().optional(),
        Param::new("end", ParamType::String).nullable().optional(),
        Param::new("count", ParamType::Int).optional(),
        Param::new("consumer", ParamType::String)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("xpending", c, o, a),
);

/// `$redis->xrange(string $key, string $start, string $end, int $count = -1)`
pub const XRANGE: MethodEntry = MethodEntry::public_async(
    "xrange",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::String),
        Param::new("end", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("xrange", c, o, a),
);

/// `$redis->xread(array $streams, int $count = -1, int $block = -1)`
pub const XREAD: MethodEntry = MethodEntry::public_async(
    "xread",
    &[
        Param::new("streams", ParamType::Array),
        Param::new("count", ParamType::Int).optional(),
        Param::new("block", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("xread", c, o, a),
);

/// `$redis->xreadgroup(string $group, string $consumer, array $streams, int $count = 1, int $block = 1)`
pub const XREADGROUP: MethodEntry = MethodEntry::public_async(
    "xreadgroup",
    &[
        Param::new("group", ParamType::String),
        Param::new("consumer", ParamType::String),
        Param::new("streams", ParamType::Array),
        Param::new("count", ParamType::Int).optional(),
        Param::new("block", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("xreadgroup", c, o, a),
);

/// `$redis->xrevrange(string $key, string $end, string $start, int $count = -1)`
pub const XREVRANGE: MethodEntry = MethodEntry::public_async(
    "xrevrange",
    &[
        Param::new("key", ParamType::String),
        Param::new("end", ParamType::String),
        Param::new("start", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("xrevrange", c, o, a),
);

/// `$redis->xtrim(string $key, string $threshold, bool $approx = false, bool $minid = false, int $limit = -1)`
pub const XTRIM: MethodEntry = MethodEntry::public_async(
    "xtrim",
    &[
        Param::new("key", ParamType::String),
        Param::new("threshold", ParamType::String),
        Param::new("approx", ParamType::Bool).optional(),
        Param::new("minid", ParamType::Bool).optional(),
        Param::new("limit", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("xtrim", c, o, a),
);
