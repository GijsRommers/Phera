//! Sorted sets: the PHP signatures, built in `crate::commands::sorted` (the blocking pops in `collections`).

use phera_extension_api::{MethodEntry, MethodPreparation, Param, ParamType};

use crate::arguments;
use crate::dispatch::dispatch;

/// `$redis->bzPopMax(array|string $key, string|int $timeout_or_key, mixed ...$extra_args)`
pub const BZ_POP_MAX: MethodEntry = MethodEntry::public_async(
    "bzPopMax",
    &[
        Param::new("key", ParamType::ArrayOrString),
        Param::new("timeout_or_key", ParamType::IntOrString),
        Param::new("extra_args", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("bzpopmax", c, o, a),
)
.with_preparation(|c, _, a| arguments::key_list("Redis::bzPopMax", BZ_POP_MAX_KEYS, c, a));
/// How phpredis parses `bzPopMax()`: any value is a key.
const BZ_POP_MAX_KEYS: &[Param] = &[
    Param::new("key", ParamType::Mixed),
    Param::new("timeout_or_key", ParamType::Mixed),
    Param::new("extra_args", ParamType::Mixed).variadic(),
];

/// `$redis->bzPopMin(array|string $key, string|int $timeout_or_key, mixed ...$extra_args)`
pub const BZ_POP_MIN: MethodEntry = MethodEntry::public_async(
    "bzPopMin",
    &[
        Param::new("key", ParamType::ArrayOrString),
        Param::new("timeout_or_key", ParamType::IntOrString),
        Param::new("extra_args", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("bzpopmin", c, o, a),
)
.with_preparation(|c, _, a| arguments::key_list("Redis::bzPopMin", BZ_POP_MIN_KEYS, c, a));
/// How phpredis parses `bzPopMin()`: any value is a key.
const BZ_POP_MIN_KEYS: &[Param] = &[
    Param::new("key", ParamType::Mixed),
    Param::new("timeout_or_key", ParamType::Mixed),
    Param::new("extra_args", ParamType::Mixed).variadic(),
];

/// `$redis->bzmpop(float $timeout, array $keys, string $from, int $count = 1)`
pub const BZMPOP: MethodEntry = MethodEntry::public_async(
    "bzmpop",
    &[
        Param::new("timeout", ParamType::Float),
        Param::new("keys", ParamType::Array),
        Param::new("from", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("bzmpop", c, o, a),
);

/// `$redis->zmpop(array $keys, string $from, int $count = 1)`
pub const ZMPOP: MethodEntry = MethodEntry::public_async(
    "zmpop",
    &[
        Param::new("keys", ParamType::Array),
        Param::new("from", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("zmpop", c, o, a),
);

/// `$redis->zAdd(string $key, array|float $score_or_options, mixed ...$more_scores_and_mems)`
pub const Z_ADD: MethodEntry = MethodEntry::public_async(
    "zAdd",
    &[
        Param::new("key", ParamType::String),
        Param::new("score_or_options", ParamType::ArrayOrFloat),
        Param::new("more_scores_and_mems", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("zadd", c, o, a),
)
.with_preparation(|_, _, a| arguments::at_least("Redis::zAdd", 3, a));

/// `$redis->zCard(string $key)`
pub const Z_CARD: MethodEntry = MethodEntry::public_async(
    "zCard",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("zcard", c, o, a),
);

/// `$redis->zCount(string $key, string|int $start, string|int $end)`
pub const Z_COUNT: MethodEntry = MethodEntry::public_async(
    "zCount",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::IntOrString),
        Param::new("end", ParamType::IntOrString),
    ],
    |c, o, a| dispatch("zcount", c, o, a),
);

/// `$redis->zIncrBy(string $key, float $value, mixed $member)`
pub const Z_INCR_BY: MethodEntry = MethodEntry::public_async(
    "zIncrBy",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Float),
        Param::new("member", ParamType::Mixed),
    ],
    |c, o, a| dispatch("zincrby", c, o, a),
);

/// `$redis->zLexCount(string $key, string $min, string $max)`
pub const Z_LEX_COUNT: MethodEntry = MethodEntry::public_async(
    "zLexCount",
    &[
        Param::new("key", ParamType::String),
        Param::new("min", ParamType::String),
        Param::new("max", ParamType::String),
    ],
    |c, o, a| dispatch("zlexcount", c, o, a),
);

/// `$redis->zMscore(string $key, mixed $member, mixed ...$other_members)`
pub const Z_MSCORE: MethodEntry = MethodEntry::public_async(
    "zMscore",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
        Param::new("other_members", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("zmscore", c, o, a),
);

/// `$redis->zPopMax(string $key, ?int $count = null)`
pub const Z_POP_MAX: MethodEntry = MethodEntry::public_async(
    "zPopMax",
    &[
        Param::new("key", ParamType::String),
        Param::new("count", ParamType::Int).nullable().optional(),
    ],
    |c, o, a| dispatch("zpopmax", c, o, a),
)
.with_preparation(|_, _, a| {
    arguments::not_empty("Redis::zPopMax", a)?;
    Ok(MethodPreparation::Default)
});

/// `$redis->zPopMin(string $key, ?int $count = null)`
pub const Z_POP_MIN: MethodEntry = MethodEntry::public_async(
    "zPopMin",
    &[
        Param::new("key", ParamType::String),
        Param::new("count", ParamType::Int).nullable().optional(),
    ],
    |c, o, a| dispatch("zpopmin", c, o, a),
)
.with_preparation(|_, _, a| {
    arguments::not_empty("Redis::zPopMin", a)?;
    Ok(MethodPreparation::Default)
});

/// `$redis->zRange(string $key, string|int $start, string|int $end, array|bool|null $options = null)`
pub const Z_RANGE: MethodEntry = MethodEntry::public_async(
    "zRange",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::IntOrString),
        Param::new("end", ParamType::IntOrString),
        Param::new("options", ParamType::ArrayOrBool)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("zrange", c, o, a),
);

/// `$redis->zRangeByLex(string $key, string $min, string $max, int $offset = -1, int $count = -1)`
pub const Z_RANGE_BY_LEX: MethodEntry = MethodEntry::public_async(
    "zRangeByLex",
    &[
        Param::new("key", ParamType::String),
        Param::new("min", ParamType::String),
        Param::new("max", ParamType::String),
        Param::new("offset", ParamType::Int).optional(),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("zrangebylex", c, o, a),
)
.with_preparation(|c, _, a| arguments::three_or_five("Redis::zRangeByLex", c, a));

/// `$redis->zRangeByScore(string $key, string $start, string $end, array $options = [])`
pub const Z_RANGE_BY_SCORE: MethodEntry = MethodEntry::public_async(
    "zRangeByScore",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::String),
        Param::new("end", ParamType::String),
        Param::new("options", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("zrangebyscore", c, o, a),
);

/// `$redis->zrangestore(string $dstkey, string $srckey, string $start, string $end, array|bool|null $options = null)`
pub const ZRANGESTORE: MethodEntry = MethodEntry::public_async(
    "zrangestore",
    &[
        Param::new("dstkey", ParamType::String),
        Param::new("srckey", ParamType::String),
        Param::new("start", ParamType::String),
        Param::new("end", ParamType::String),
        Param::new("options", ParamType::ArrayOrBool)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("zrangestore", c, o, a),
);

/// `$redis->zRandMember(string $key, ?array $options = null)`
pub const Z_RAND_MEMBER: MethodEntry = MethodEntry::public_async(
    "zRandMember",
    &[
        Param::new("key", ParamType::String),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("zrandmember", c, o, a),
);

/// `$redis->zRank(string $key, mixed $member)`
pub const Z_RANK: MethodEntry = MethodEntry::public_async(
    "zRank",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
    ],
    |c, o, a| dispatch("zrank", c, o, a),
);

/// `$redis->zRem(mixed $key, mixed $member, mixed ...$other_members)`
pub const Z_REM: MethodEntry = MethodEntry::public_async(
    "zRem",
    &[
        Param::new("key", ParamType::Mixed),
        Param::new("member", ParamType::Mixed),
        Param::new("other_members", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("zrem", c, o, a),
);

/// `$redis->zRemRangeByLex(string $key, string $min, string $max)`
pub const Z_REM_RANGE_BY_LEX: MethodEntry = MethodEntry::public_async(
    "zRemRangeByLex",
    &[
        Param::new("key", ParamType::String),
        Param::new("min", ParamType::String),
        Param::new("max", ParamType::String),
    ],
    |c, o, a| dispatch("zremrangebylex", c, o, a),
);

/// `$redis->zRemRangeByRank(string $key, int $start, int $end)`
pub const Z_REM_RANGE_BY_RANK: MethodEntry = MethodEntry::public_async(
    "zRemRangeByRank",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::Int),
        Param::new("end", ParamType::Int),
    ],
    |c, o, a| dispatch("zremrangebyrank", c, o, a),
);

/// `$redis->zRemRangeByScore(string $key, string $start, string $end)`
pub const Z_REM_RANGE_BY_SCORE: MethodEntry = MethodEntry::public_async(
    "zRemRangeByScore",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::String),
        Param::new("end", ParamType::String),
    ],
    |c, o, a| dispatch("zremrangebyscore", c, o, a),
);

/// `$redis->zRevRange(string $key, int $start, int $end, mixed $scores = null)`
pub const Z_REV_RANGE: MethodEntry = MethodEntry::public_async(
    "zRevRange",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::Int),
        Param::new("end", ParamType::Int),
        Param::new("scores", ParamType::Mixed).optional(),
    ],
    |c, o, a| dispatch("zrevrange", c, o, a),
);

/// `$redis->zRevRangeByLex(string $key, string $max, string $min, int $offset = -1, int $count = -1)`
pub const Z_REV_RANGE_BY_LEX: MethodEntry = MethodEntry::public_async(
    "zRevRangeByLex",
    &[
        Param::new("key", ParamType::String),
        Param::new("max", ParamType::String),
        Param::new("min", ParamType::String),
        Param::new("offset", ParamType::Int).optional(),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("zrevrangebylex", c, o, a),
)
.with_preparation(|c, _, a| arguments::three_or_five("Redis::zRevRangeByLex", c, a));

/// `$redis->zRevRangeByScore(string $key, string $max, string $min, array|bool $options = [])`
pub const Z_REV_RANGE_BY_SCORE: MethodEntry = MethodEntry::public_async(
    "zRevRangeByScore",
    &[
        Param::new("key", ParamType::String),
        Param::new("max", ParamType::String),
        Param::new("min", ParamType::String),
        Param::new("options", ParamType::ArrayOrBool).optional(),
    ],
    |c, o, a| dispatch("zrevrangebyscore", c, o, a),
);

/// `$redis->zRevRank(string $key, mixed $member)`
pub const Z_REV_RANK: MethodEntry = MethodEntry::public_async(
    "zRevRank",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
    ],
    |c, o, a| dispatch("zrevrank", c, o, a),
);

/// `$redis->zScore(string $key, mixed $member)`
pub const Z_SCORE: MethodEntry = MethodEntry::public_async(
    "zScore",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
    ],
    |c, o, a| dispatch("zscore", c, o, a),
);

/// `$redis->zdiff(array $keys, ?array $options = null)`
pub const ZDIFF: MethodEntry = MethodEntry::public_async(
    "zdiff",
    &[
        Param::new("keys", ParamType::Array),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("zdiff", c, o, a),
);

/// `$redis->zdiffstore(string $dst, array $keys)`
pub const ZDIFFSTORE: MethodEntry = MethodEntry::public_async(
    "zdiffstore",
    &[
        Param::new("dst", ParamType::String),
        Param::new("keys", ParamType::Array),
    ],
    |c, o, a| dispatch("zdiffstore", c, o, a),
);

/// `$redis->zinter(array $keys, ?array $weights = null, ?array $options = null)`
pub const ZINTER: MethodEntry = MethodEntry::public_async(
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
);

/// `$redis->zintercard(array $keys, int $limit = -1)`
pub const ZINTERCARD: MethodEntry = MethodEntry::public_async(
    "zintercard",
    &[
        Param::new("keys", ParamType::Array),
        Param::new("limit", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("zintercard", c, o, a),
);

/// `$redis->zinterstore(string $dst, array $keys, ?array $weights = null, ?string $aggregate = null)`
pub const ZINTERSTORE: MethodEntry = MethodEntry::public_async(
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
);

/// `$redis->zunion(array $keys, ?array $weights = null, ?array $options = null)`
pub const ZUNION: MethodEntry = MethodEntry::public_async(
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
);

/// `$redis->zunionstore(string $dst, array $keys, ?array $weights = null, ?string $aggregate = null)`
pub const ZUNIONSTORE: MethodEntry = MethodEntry::public_async(
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
);
