//! Lists: the PHP signatures, built in `crate::commands::collections`.

use phera_extension_api::{MethodEntry, Param, ParamType};

use crate::arguments;
use crate::dispatch::dispatch;

/// `$redis->blPop(array|string $key_or_keys, string|int|float $timeout_or_key, mixed ...$extra_args)`
pub const BL_POP: MethodEntry = MethodEntry::public_async(
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
.with_preparation(|c, _, a| arguments::key_list("Redis::blPop", BL_POP_KEYS, c, a));
/// How phpredis parses `blPop()`: any value is a key.
const BL_POP_KEYS: &[Param] = &[
    Param::new("key_or_keys", ParamType::Mixed),
    Param::new("timeout_or_key", ParamType::Mixed),
    Param::new("extra_args", ParamType::Mixed).variadic(),
];

/// `$redis->brPop(array|string $key_or_keys, string|int|float $timeout_or_key, mixed ...$extra_args)`
pub const BR_POP: MethodEntry = MethodEntry::public_async(
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
.with_preparation(|c, _, a| arguments::key_list("Redis::brPop", BR_POP_KEYS, c, a));
/// How phpredis parses `brPop()`: any value is a key.
const BR_POP_KEYS: &[Param] = &[
    Param::new("key_or_keys", ParamType::Mixed),
    Param::new("timeout_or_key", ParamType::Mixed),
    Param::new("extra_args", ParamType::Mixed).variadic(),
];

/// `$redis->brpoplpush(string $src, string $dst, int|float $timeout)`
pub const BRPOPLPUSH: MethodEntry = MethodEntry::public_async(
    "brpoplpush",
    &[
        Param::new("src", ParamType::String),
        Param::new("dst", ParamType::String),
        Param::new("timeout", ParamType::Number),
    ],
    |c, o, a| dispatch("brpoplpush", c, o, a),
);

/// `$redis->blmpop(float $timeout, array $keys, string $from, int $count = 1)`
pub const BLMPOP: MethodEntry = MethodEntry::public_async(
    "blmpop",
    &[
        Param::new("timeout", ParamType::Float),
        Param::new("keys", ParamType::Array),
        Param::new("from", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("blmpop", c, o, a),
);

/// `$redis->lmpop(array $keys, string $from, int $count = 1)`
pub const LMPOP: MethodEntry = MethodEntry::public_async(
    "lmpop",
    &[
        Param::new("keys", ParamType::Array),
        Param::new("from", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("lmpop", c, o, a),
);

/// `$redis->expiremember(string $key, string $field, int $ttl, ?string $unit = null)`
pub const EXPIREMEMBER: MethodEntry = MethodEntry::public_async(
    "expiremember",
    &[
        Param::new("key", ParamType::String),
        Param::new("field", ParamType::String),
        Param::new("ttl", ParamType::Int),
        Param::new("unit", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("expiremember", c, o, a),
);

/// `$redis->expirememberat(string $key, string $field, int $timestamp)`
pub const EXPIREMEMBERAT: MethodEntry = MethodEntry::public_async(
    "expirememberat",
    &[
        Param::new("key", ParamType::String),
        Param::new("field", ParamType::String),
        Param::new("timestamp", ParamType::Int),
    ],
    |c, o, a| dispatch("expirememberat", c, o, a),
);

/// `$redis->lInsert(string $key, string $pos, mixed $pivot, mixed $value)`
pub const L_INSERT: MethodEntry = MethodEntry::public_async(
    "lInsert",
    &[
        Param::new("key", ParamType::String),
        Param::new("pos", ParamType::String),
        Param::new("pivot", ParamType::Mixed),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("linsert", c, o, a),
);

/// `$redis->lLen(string $key)`
pub const L_LEN: MethodEntry = MethodEntry::public_async(
    "lLen",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("llen", c, o, a),
);

/// `$redis->lMove(string $src, string $dst, string $wherefrom, string $whereto)`
pub const L_MOVE: MethodEntry = MethodEntry::public_async(
    "lMove",
    &[
        Param::new("src", ParamType::String),
        Param::new("dst", ParamType::String),
        Param::new("wherefrom", ParamType::String),
        Param::new("whereto", ParamType::String),
    ],
    |c, o, a| dispatch("lmove", c, o, a),
);

/// `$redis->blmove(string $src, string $dst, string $wherefrom, string $whereto, float $timeout)`
pub const BLMOVE: MethodEntry = MethodEntry::public_async(
    "blmove",
    &[
        Param::new("src", ParamType::String),
        Param::new("dst", ParamType::String),
        Param::new("wherefrom", ParamType::String),
        Param::new("whereto", ParamType::String),
        Param::new("timeout", ParamType::Float),
    ],
    |c, o, a| dispatch("blmove", c, o, a),
);

/// `$redis->lPop(string $key, int $count = 0)`
pub const L_POP: MethodEntry = MethodEntry::public_async(
    "lPop",
    &[
        Param::new("key", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("lpop", c, o, a),
);

/// `$redis->lPos(string $key, mixed $value, ?array $options = null)`
pub const L_POS: MethodEntry = MethodEntry::public_async(
    "lPos",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("lpos", c, o, a),
);

/// `$redis->lPush(string $key, mixed ...$elements)`
pub const L_PUSH: MethodEntry = MethodEntry::public_async(
    "lPush",
    &[
        Param::new("key", ParamType::String),
        Param::new("elements", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("lpush", c, o, a),
)
.with_preparation(|_, _, a| arguments::at_least("Redis::lPush", 2, a));

/// `$redis->rPush(string $key, mixed ...$elements)`
pub const R_PUSH: MethodEntry = MethodEntry::public_async(
    "rPush",
    &[
        Param::new("key", ParamType::String),
        Param::new("elements", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("rpush", c, o, a),
)
.with_preparation(|_, _, a| arguments::at_least("Redis::rPush", 2, a));

/// `$redis->lPushx(string $key, mixed $value)`
pub const L_PUSHX: MethodEntry = MethodEntry::public_async(
    "lPushx",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("lpushx", c, o, a),
);

/// `$redis->rPushx(string $key, mixed $value)`
pub const R_PUSHX: MethodEntry = MethodEntry::public_async(
    "rPushx",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("rpushx", c, o, a),
);

/// `$redis->lSet(string $key, int $index, mixed $value)`
pub const L_SET: MethodEntry = MethodEntry::public_async(
    "lSet",
    &[
        Param::new("key", ParamType::String),
        Param::new("index", ParamType::Int),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("lset", c, o, a),
);

/// `$redis->lindex(string $key, int $index)`
pub const LINDEX: MethodEntry = MethodEntry::public_async(
    "lindex",
    &[
        Param::new("key", ParamType::String),
        Param::new("index", ParamType::Int),
    ],
    |c, o, a| dispatch("lindex", c, o, a),
);

/// `$redis->lrange(string $key, int $start, int $end)`
pub const LRANGE: MethodEntry = MethodEntry::public_async(
    "lrange",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::Int),
        Param::new("end", ParamType::Int),
    ],
    |c, o, a| dispatch("lrange", c, o, a),
);

/// `$redis->lrem(string $key, mixed $value, int $count = 0)`
pub const LREM: MethodEntry = MethodEntry::public_async(
    "lrem",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("lrem", c, o, a),
);

/// `$redis->ltrim(string $key, int $start, int $end)`
pub const LTRIM: MethodEntry = MethodEntry::public_async(
    "ltrim",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::Int),
        Param::new("end", ParamType::Int),
    ],
    |c, o, a| dispatch("ltrim", c, o, a),
);

/// `$redis->rPop(string $key, int $count = 0)`
pub const R_POP: MethodEntry = MethodEntry::public_async(
    "rPop",
    &[
        Param::new("key", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("rpop", c, o, a),
);

/// `$redis->rpoplpush(string $srckey, string $dstkey)`
pub const RPOPLPUSH: MethodEntry = MethodEntry::public_async(
    "rpoplpush",
    &[
        Param::new("srckey", ParamType::String),
        Param::new("dstkey", ParamType::String),
    ],
    |c, o, a| dispatch("rpoplpush", c, o, a),
);
