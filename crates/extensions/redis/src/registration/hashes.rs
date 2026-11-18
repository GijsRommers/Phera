//! Hashes: the PHP signatures, built in `crate::commands::collections`.

use phera_extension_api::{MethodEntry, Param, ParamType};

use crate::arguments;
use crate::dispatch::dispatch;

/// `$redis->hDel(string $key, string $field, string ...$other_fields)`
pub const H_DEL: MethodEntry = MethodEntry::public_async(
    "hDel",
    &[
        Param::new("key", ParamType::String),
        Param::new("field", ParamType::String),
        Param::new("other_fields", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("hdel", c, o, a),
);

/// `$redis->hExists(string $key, string $field)`
pub const H_EXISTS: MethodEntry = MethodEntry::public_async(
    "hExists",
    &[
        Param::new("key", ParamType::String),
        Param::new("field", ParamType::String),
    ],
    |c, o, a| dispatch("hexists", c, o, a),
);

/// `$redis->hGet(string $key, string $member)`
pub const H_GET: MethodEntry = MethodEntry::public_async(
    "hGet",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::String),
    ],
    |c, o, a| dispatch("hget", c, o, a),
);

/// `$redis->hGetAll(string $key)`
pub const H_GET_ALL: MethodEntry = MethodEntry::public_async(
    "hGetAll",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("hgetall", c, o, a),
);

/// `$redis->hGetWithMeta(string $key, string $member)`
pub const H_GET_WITH_META: MethodEntry = MethodEntry::public_async(
    "hGetWithMeta",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::String),
    ],
    |c, o, a| dispatch("hgetwithmeta", c, o, a),
);

/// `$redis->hIncrBy(string $key, string $field, int $value)`
pub const H_INCR_BY: MethodEntry = MethodEntry::public_async(
    "hIncrBy",
    &[
        Param::new("key", ParamType::String),
        Param::new("field", ParamType::String),
        Param::new("value", ParamType::Int),
    ],
    |c, o, a| dispatch("hincrby", c, o, a),
);

/// `$redis->hIncrByFloat(string $key, string $field, float $value)`
pub const H_INCR_BY_FLOAT: MethodEntry = MethodEntry::public_async(
    "hIncrByFloat",
    &[
        Param::new("key", ParamType::String),
        Param::new("field", ParamType::String),
        Param::new("value", ParamType::Float),
    ],
    |c, o, a| dispatch("hincrbyfloat", c, o, a),
);

/// `$redis->hKeys(string $key)`
pub const H_KEYS: MethodEntry = MethodEntry::public_async(
    "hKeys",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("hkeys", c, o, a),
);

/// `$redis->hLen(string $key)`
pub const H_LEN: MethodEntry = MethodEntry::public_async(
    "hLen",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("hlen", c, o, a),
);

/// `$redis->hMget(string $key, array $fields)`
pub const H_MGET: MethodEntry = MethodEntry::public_async(
    "hMget",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
    ],
    |c, o, a| dispatch("hmget", c, o, a),
);

/// `$redis->hgetex(string $key, array $fields, array|string|null $expiry = null)`
pub const HGETEX: MethodEntry = MethodEntry::public_async(
    "hgetex",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
        Param::new("expiry", ParamType::ArrayOrString)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("hgetex", c, o, a),
);

/// `$redis->hsetex(string $key, array $fields, ?array $expiry = null)`
pub const HSETEX: MethodEntry = MethodEntry::public_async(
    "hsetex",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
        Param::new("expiry", ParamType::Array).nullable().optional(),
    ],
    |c, o, a| dispatch("hsetex", c, o, a),
);

/// `$redis->hgetdel(string $key, array $fields)`
pub const HGETDEL: MethodEntry = MethodEntry::public_async(
    "hgetdel",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
    ],
    |c, o, a| dispatch("hgetdel", c, o, a),
);

/// `$redis->hMset(string $key, array $fieldvals)`
pub const H_MSET: MethodEntry = MethodEntry::public_async(
    "hMset",
    &[
        Param::new("key", ParamType::String),
        Param::new("fieldvals", ParamType::Array),
    ],
    |c, o, a| dispatch("hmset", c, o, a),
);

/// `$redis->hRandField(string $key, ?array $options = null)`
pub const H_RAND_FIELD: MethodEntry = MethodEntry::public_async(
    "hRandField",
    &[
        Param::new("key", ParamType::String),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("hrandfield", c, o, a),
);

/// `$redis->hSet(string $key, mixed ...$fields_and_vals)`
pub const H_SET: MethodEntry = MethodEntry::public_async(
    "hSet",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields_and_vals", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("hset", c, o, a),
)
.with_preparation(|_, _, a| arguments::at_least("Redis::hSet", 2, a));

/// `$redis->hSetNx(string $key, string $field, mixed $value)`
pub const H_SET_NX: MethodEntry = MethodEntry::public_async(
    "hSetNx",
    &[
        Param::new("key", ParamType::String),
        Param::new("field", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("hsetnx", c, o, a),
);

/// `$redis->hStrLen(string $key, string $field)`
pub const H_STR_LEN: MethodEntry = MethodEntry::public_async(
    "hStrLen",
    &[
        Param::new("key", ParamType::String),
        Param::new("field", ParamType::String),
    ],
    |c, o, a| dispatch("hstrlen", c, o, a),
);

/// `$redis->hVals(string $key)`
pub const H_VALS: MethodEntry = MethodEntry::public_async(
    "hVals",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("hvals", c, o, a),
);

/// `$redis->hexpire(string $key, int $ttl, array $fields, ?string $mode = null)`
pub const HEXPIRE: MethodEntry = MethodEntry::public_async(
    "hexpire",
    &[
        Param::new("key", ParamType::String),
        Param::new("ttl", ParamType::Int),
        Param::new("fields", ParamType::Array),
        Param::new("mode", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("hexpire", c, o, a),
);

/// `$redis->hpexpire(string $key, int $ttl, array $fields, ?string $mode = null)`
pub const HPEXPIRE: MethodEntry = MethodEntry::public_async(
    "hpexpire",
    &[
        Param::new("key", ParamType::String),
        Param::new("ttl", ParamType::Int),
        Param::new("fields", ParamType::Array),
        Param::new("mode", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("hpexpire", c, o, a),
);

/// `$redis->hexpireat(string $key, int $time, array $fields, ?string $mode = null)`
pub const HEXPIREAT: MethodEntry = MethodEntry::public_async(
    "hexpireat",
    &[
        Param::new("key", ParamType::String),
        Param::new("time", ParamType::Int),
        Param::new("fields", ParamType::Array),
        Param::new("mode", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("hexpireat", c, o, a),
);

/// `$redis->hpexpireat(string $key, int $mstime, array $fields, ?string $mode = null)`
pub const HPEXPIREAT: MethodEntry = MethodEntry::public_async(
    "hpexpireat",
    &[
        Param::new("key", ParamType::String),
        Param::new("mstime", ParamType::Int),
        Param::new("fields", ParamType::Array),
        Param::new("mode", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("hpexpireat", c, o, a),
);

/// `$redis->httl(string $key, array $fields)`
pub const HTTL: MethodEntry = MethodEntry::public_async(
    "httl",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
    ],
    |c, o, a| dispatch("httl", c, o, a),
);

/// `$redis->hpttl(string $key, array $fields)`
pub const HPTTL: MethodEntry = MethodEntry::public_async(
    "hpttl",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
    ],
    |c, o, a| dispatch("hpttl", c, o, a),
);

/// `$redis->hexpiretime(string $key, array $fields)`
pub const HEXPIRETIME: MethodEntry = MethodEntry::public_async(
    "hexpiretime",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
    ],
    |c, o, a| dispatch("hexpiretime", c, o, a),
);

/// `$redis->hpexpiretime(string $key, array $fields)`
pub const HPEXPIRETIME: MethodEntry = MethodEntry::public_async(
    "hpexpiretime",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
    ],
    |c, o, a| dispatch("hpexpiretime", c, o, a),
);

/// `$redis->hpersist(string $key, array $fields)`
pub const HPERSIST: MethodEntry = MethodEntry::public_async(
    "hpersist",
    &[
        Param::new("key", ParamType::String),
        Param::new("fields", ParamType::Array),
    ],
    |c, o, a| dispatch("hpersist", c, o, a),
);
