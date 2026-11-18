//! Sets: the PHP signatures, built in `crate::commands::collections`.

use phera_extension_api::{MethodEntry, MethodPreparation, Param, ParamType};

use crate::arguments;
use crate::dispatch::dispatch;

/// `$redis->sAdd(string $key, mixed $value, mixed ...$other_values)`
pub const S_ADD: MethodEntry = MethodEntry::public_async(
    "sAdd",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
        Param::new("other_values", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("sadd", c, o, a),
);

/// `$redis->sAddArray(string $key, array $values)`
pub const S_ADD_ARRAY: MethodEntry = MethodEntry::public_async(
    "sAddArray",
    &[
        Param::new("key", ParamType::String),
        Param::new("values", ParamType::Array),
    ],
    |c, o, a| dispatch("saddarray", c, o, a),
);

/// `$redis->sDiff(string $key, string ...$other_keys)`
pub const S_DIFF: MethodEntry = MethodEntry::public_async(
    "sDiff",
    &[
        Param::new("key", ParamType::String),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("sdiff", c, o, a),
)
.with_preparation(|c, _, a| arguments::key_list("Redis::sDiff", S_DIFF_KEYS, c, a));
/// How phpredis parses `sDiff()`: any value is a key.
const S_DIFF_KEYS: &[Param] = &[
    Param::new("key", ParamType::Mixed),
    Param::new("other_keys", ParamType::Mixed).variadic(),
];

/// `$redis->sDiffStore(string $dst, string $key, string ...$other_keys)`
pub const S_DIFF_STORE: MethodEntry = MethodEntry::public_async(
    "sDiffStore",
    &[
        Param::new("dst", ParamType::String),
        Param::new("key", ParamType::String),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("sdiffstore", c, o, a),
)
.with_preparation(|c, _, a| arguments::key_list("Redis::sDiffStore", S_DIFF_STORE_KEYS, c, a));
/// How phpredis parses `sDiffStore()`: any value is a key.
const S_DIFF_STORE_KEYS: &[Param] = &[
    Param::new("dst", ParamType::Mixed),
    Param::new("key", ParamType::Mixed).optional(),
    Param::new("other_keys", ParamType::Mixed).variadic(),
];

/// `$redis->sInter(array|string $key, string ...$other_keys)`
pub const S_INTER: MethodEntry = MethodEntry::public_async(
    "sInter",
    &[
        Param::new("key", ParamType::ArrayOrString),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("sinter", c, o, a),
)
.with_preparation(|c, _, a| arguments::key_list("Redis::sInter", S_INTER_KEYS, c, a));
/// How phpredis parses `sInter()`: any value is a key.
const S_INTER_KEYS: &[Param] = &[
    Param::new("key", ParamType::Mixed),
    Param::new("other_keys", ParamType::Mixed).variadic(),
];

/// `$redis->sintercard(array $keys, int $limit = -1)`
pub const SINTERCARD: MethodEntry = MethodEntry::public_async(
    "sintercard",
    &[
        Param::new("keys", ParamType::Array),
        Param::new("limit", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("sintercard", c, o, a),
);

/// `$redis->sInterStore(array|string $key, string ...$other_keys)`
pub const S_INTER_STORE: MethodEntry = MethodEntry::public_async(
    "sInterStore",
    &[
        Param::new("key", ParamType::ArrayOrString),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("sinterstore", c, o, a),
)
.with_preparation(|c, _, a| arguments::key_list("Redis::sInterStore", S_INTER_STORE_KEYS, c, a));
/// How phpredis parses `sInterStore()`: any value is a key.
const S_INTER_STORE_KEYS: &[Param] = &[
    Param::new("key", ParamType::Mixed),
    Param::new("other_keys", ParamType::Mixed).variadic(),
];

/// `$redis->sMembers(string $key)`
pub const S_MEMBERS: MethodEntry = MethodEntry::public_async(
    "sMembers",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("smembers", c, o, a),
);

/// `$redis->sMisMember(string $key, string $member, string ...$other_members)`
pub const S_MIS_MEMBER: MethodEntry = MethodEntry::public_async(
    "sMisMember",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::String),
        Param::new("other_members", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("smismember", c, o, a),
)
.with_preparation(|c, _, a| arguments::parse_as("Redis::sMisMember", S_MIS_MEMBER_PARSED, c, a));
/// How phpredis parses `sMisMember()`, which differs from its declaration.
const S_MIS_MEMBER_PARSED: &[Param] = &[
    Param::new("key", ParamType::String),
    Param::new("member", ParamType::Mixed),
    Param::new("other_members", ParamType::Mixed).variadic(),
];

/// `$redis->sMove(string $src, string $dst, mixed $value)`
pub const S_MOVE: MethodEntry = MethodEntry::public_async(
    "sMove",
    &[
        Param::new("src", ParamType::String),
        Param::new("dst", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("smove", c, o, a),
);

/// `$redis->sPop(string $key, int $count = 0)`
pub const S_POP: MethodEntry = MethodEntry::public_async(
    "sPop",
    &[
        Param::new("key", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("spop", c, o, a),
)
.with_preparation(|_, _, a| {
    arguments::not_empty("Redis::sPop", a)?;
    Ok(MethodPreparation::Default)
});

/// `$redis->sRandMember(string $key, int $count = 0)`
pub const S_RAND_MEMBER: MethodEntry = MethodEntry::public_async(
    "sRandMember",
    &[
        Param::new("key", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("srandmember", c, o, a),
);

/// `$redis->sUnion(string $key, string ...$other_keys)`
pub const S_UNION: MethodEntry = MethodEntry::public_async(
    "sUnion",
    &[
        Param::new("key", ParamType::String),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("sunion", c, o, a),
)
.with_preparation(|c, _, a| arguments::key_list("Redis::sUnion", S_UNION_KEYS, c, a));
/// How phpredis parses `sUnion()`: any value is a key.
const S_UNION_KEYS: &[Param] = &[
    Param::new("key", ParamType::Mixed),
    Param::new("other_keys", ParamType::Mixed).variadic(),
];

/// `$redis->sUnionStore(string $dst, string $key, string ...$other_keys)`
pub const S_UNION_STORE: MethodEntry = MethodEntry::public_async(
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
.with_preparation(|c, _, a| arguments::key_list("Redis::sUnionStore", S_UNION_STORE_KEYS, c, a));
/// How phpredis parses `sUnionStore()`: any value is a key.
const S_UNION_STORE_KEYS: &[Param] = &[
    Param::new("dst", ParamType::Mixed),
    Param::new("key", ParamType::Mixed).optional(),
    Param::new("other_keys", ParamType::Mixed).variadic(),
];

/// `$redis->scard(string $key)`
pub const SCARD: MethodEntry = MethodEntry::public_async(
    "scard",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("scard", c, o, a),
);

/// `$redis->sismember(string $key, mixed $value)`
pub const SISMEMBER: MethodEntry = MethodEntry::public_async(
    "sismember",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("sismember", c, o, a),
);

/// `$redis->srem(string $key, mixed $value, mixed ...$other_values)`
pub const SREM: MethodEntry = MethodEntry::public_async(
    "srem",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
        Param::new("other_values", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("srem", c, o, a),
);
