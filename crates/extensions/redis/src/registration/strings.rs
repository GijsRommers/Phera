//! Strings and keys: the PHP signatures, built in `crate::commands::strings`.

use phera_extension_api::{MethodEntry, Param, ParamType};

use crate::arguments;
use crate::dispatch::dispatch;

/// `$redis->append(string $key, mixed $value)`
pub const APPEND: MethodEntry = MethodEntry::public_async(
    "append",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("append", c, o, a),
);

/// `$redis->bitcount(string $key, int $start = 0, int $end = -1, bool $bybit = false)`
pub const BITCOUNT: MethodEntry = MethodEntry::public_async(
    "bitcount",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::Int).optional(),
        Param::new("end", ParamType::Int).optional(),
        Param::new("bybit", ParamType::Bool).optional(),
    ],
    |c, o, a| dispatch("bitcount", c, o, a),
);

/// `$redis->bitop(string $operation, string $deskey, string $srckey, string ...$other_keys)`
pub const BITOP: MethodEntry = MethodEntry::public_async(
    "bitop",
    &[
        Param::new("operation", ParamType::String),
        Param::new("deskey", ParamType::String),
        Param::new("srckey", ParamType::String),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("bitop", c, o, a),
)
.with_preparation(|_, _, a| arguments::bitop(a));

/// `$redis->bitpos(string $key, bool $bit, int $start = 0, int $end = -1, bool $bybit = false)`
pub const BITPOS: MethodEntry = MethodEntry::public_async(
    "bitpos",
    &[
        Param::new("key", ParamType::String),
        Param::new("bit", ParamType::Bool),
        Param::new("start", ParamType::Int).optional(),
        Param::new("end", ParamType::Int).optional(),
        Param::new("bybit", ParamType::Bool).optional(),
    ],
    |c, o, a| dispatch("bitpos", c, o, a),
);

/// `$redis->copy(string $src, string $dst, ?array $options = null)`
pub const COPY: MethodEntry = MethodEntry::public_async(
    "copy",
    &[
        Param::new("src", ParamType::String),
        Param::new("dst", ParamType::String),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("copy", c, o, a),
);

/// `$redis->decr(string $key, int $by = 1)`
pub const DECR: MethodEntry = MethodEntry::public_async(
    "decr",
    &[
        Param::new("key", ParamType::String),
        Param::new("by", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("decr", c, o, a),
);

/// `$redis->decrBy(string $key, int $value)`
pub const DECR_BY: MethodEntry = MethodEntry::public_async(
    "decrBy",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Int),
    ],
    |c, o, a| dispatch("decrby", c, o, a),
);

/// `$redis->del(array|string $key, string ...$other_keys)`
pub const DEL: MethodEntry = MethodEntry::public_async(
    "del",
    &[
        Param::new("key", ParamType::ArrayOrString),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("del", c, o, a),
)
.with_preparation(|c, _, a| arguments::key_list("Redis::del", DEL_KEYS, c, a));
/// How phpredis parses `del()`: any value is a key.
const DEL_KEYS: &[Param] = &[
    Param::new("key", ParamType::Mixed),
    Param::new("other_keys", ParamType::Mixed).variadic(),
];

/// `$redis->delifeq(string $key, mixed $value)`
pub const DELIFEQ: MethodEntry = MethodEntry::public_async(
    "delifeq",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("delifeq", c, o, a),
);

/// `$redis->delete(array|string $key, string ...$other_keys)`
pub const DELETE: MethodEntry = MethodEntry::public_async(
    "delete",
    &[
        Param::new("key", ParamType::ArrayOrString),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("delete", c, o, a),
)
.with_preparation(|c, _, a| {
    arguments::deprecated("Redis::delete", c);
    arguments::key_list("Redis::delete", DELETE_KEYS, c, a)
});
/// How phpredis parses `delete()`: any value is a key.
const DELETE_KEYS: &[Param] = &[
    Param::new("key", ParamType::Mixed),
    Param::new("other_keys", ParamType::Mixed).variadic(),
];

/// `$redis->dump(string $key)`
pub const DUMP: MethodEntry = MethodEntry::public_async(
    "dump",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("dump", c, o, a),
);

/// `$redis->exists(mixed $key, mixed ...$other_keys)`
pub const EXISTS: MethodEntry = MethodEntry::public_async(
    "exists",
    &[
        Param::new("key", ParamType::Mixed),
        Param::new("other_keys", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("exists", c, o, a),
)
.with_preparation(|c, _, a| arguments::key_list("Redis::exists", EXISTS_KEYS, c, a));
/// How phpredis parses `exists()`: any value is a key.
const EXISTS_KEYS: &[Param] = &[
    Param::new("key", ParamType::Mixed),
    Param::new("other_keys", ParamType::Mixed).variadic(),
];

/// `$redis->expire(string $key, int $timeout, ?string $mode = null)`
pub const EXPIRE: MethodEntry = MethodEntry::public_async(
    "expire",
    &[
        Param::new("key", ParamType::String),
        Param::new("timeout", ParamType::Int),
        Param::new("mode", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("expire", c, o, a),
);

/// `$redis->expireAt(string $key, int $timestamp, ?string $mode = null)`
pub const EXPIRE_AT: MethodEntry = MethodEntry::public_async(
    "expireAt",
    &[
        Param::new("key", ParamType::String),
        Param::new("timestamp", ParamType::Int),
        Param::new("mode", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("expireat", c, o, a),
);

/// `$redis->expiretime(string $key)`
pub const EXPIRETIME: MethodEntry = MethodEntry::public_async(
    "expiretime",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("expiretime", c, o, a),
);

/// `$redis->pexpiretime(string $key)`
pub const PEXPIRETIME: MethodEntry = MethodEntry::public_async(
    "pexpiretime",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("pexpiretime", c, o, a),
);

/// `$redis->get(string $key)`
pub const GET: MethodEntry =
    MethodEntry::public_async("get", &[Param::new("key", ParamType::String)], |c, o, a| {
        dispatch("get", c, o, a)
    });

/// `$redis->getWithMeta(string $key)`
pub const GET_WITH_META: MethodEntry = MethodEntry::public_async(
    "getWithMeta",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("getwithmeta", c, o, a),
);

/// `$redis->getBit(string $key, int $idx)`
pub const GET_BIT: MethodEntry = MethodEntry::public_async(
    "getBit",
    &[
        Param::new("key", ParamType::String),
        Param::new("idx", ParamType::Int),
    ],
    |c, o, a| dispatch("getbit", c, o, a),
);

/// `$redis->getEx(string $key, array $options = [])`
pub const GET_EX: MethodEntry = MethodEntry::public_async(
    "getEx",
    &[
        Param::new("key", ParamType::String),
        Param::new("options", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("getex", c, o, a),
);

/// `$redis->getDel(string $key)`
pub const GET_DEL: MethodEntry = MethodEntry::public_async(
    "getDel",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("getdel", c, o, a),
);

/// `$redis->getRange(string $key, int $start, int $end)`
pub const GET_RANGE: MethodEntry = MethodEntry::public_async(
    "getRange",
    &[
        Param::new("key", ParamType::String),
        Param::new("start", ParamType::Int),
        Param::new("end", ParamType::Int),
    ],
    |c, o, a| dispatch("getrange", c, o, a),
);

/// `$redis->lcs(string $key1, string $key2, ?array $options = null)`
pub const LCS: MethodEntry = MethodEntry::public_async(
    "lcs",
    &[
        Param::new("key1", ParamType::String),
        Param::new("key2", ParamType::String),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("lcs", c, o, a),
);

/// `$redis->getset(string $key, mixed $value)`
pub const GETSET: MethodEntry = MethodEntry::public_async(
    "getset",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("getset", c, o, a),
);

/// `$redis->incr(string $key, int $by = 1)`
pub const INCR: MethodEntry = MethodEntry::public_async(
    "incr",
    &[
        Param::new("key", ParamType::String),
        Param::new("by", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("incr", c, o, a),
);

/// `$redis->incrBy(string $key, int $value)`
pub const INCR_BY: MethodEntry = MethodEntry::public_async(
    "incrBy",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Int),
    ],
    |c, o, a| dispatch("incrby", c, o, a),
);

/// `$redis->incrByFloat(string $key, float $value)`
pub const INCR_BY_FLOAT: MethodEntry = MethodEntry::public_async(
    "incrByFloat",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Float),
    ],
    |c, o, a| dispatch("incrbyfloat", c, o, a),
);

/// `$redis->keys(string $pattern)`
pub const KEYS: MethodEntry = MethodEntry::public_async(
    "keys",
    &[Param::new("pattern", ParamType::String)],
    |c, o, a| dispatch("keys", c, o, a),
);

/// `$redis->mget(array $keys)`
pub const MGET: MethodEntry = MethodEntry::public_async(
    "mget",
    &[Param::new("keys", ParamType::Array)],
    |c, o, a| dispatch("mget", c, o, a),
);

/// `$redis->mset(array $key_values)`
pub const MSET: MethodEntry = MethodEntry::public_async(
    "mset",
    &[Param::new("key_values", ParamType::Array)],
    |c, o, a| dispatch("mset", c, o, a),
);

/// `$redis->msetnx(array $key_values)`
pub const MSETNX: MethodEntry = MethodEntry::public_async(
    "msetnx",
    &[Param::new("key_values", ParamType::Array)],
    |c, o, a| dispatch("msetnx", c, o, a),
);

/// `$redis->persist(string $key)`
pub const PERSIST: MethodEntry = MethodEntry::public_async(
    "persist",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("persist", c, o, a),
);

/// `$redis->pexpire(string $key, int $timeout, ?string $mode = null)`
pub const PEXPIRE: MethodEntry = MethodEntry::public_async(
    "pexpire",
    &[
        Param::new("key", ParamType::String),
        Param::new("timeout", ParamType::Int),
        Param::new("mode", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("pexpire", c, o, a),
);

/// `$redis->pexpireAt(string $key, int $timestamp, ?string $mode = null)`
pub const PEXPIRE_AT: MethodEntry = MethodEntry::public_async(
    "pexpireAt",
    &[
        Param::new("key", ParamType::String),
        Param::new("timestamp", ParamType::Int),
        Param::new("mode", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("pexpireat", c, o, a),
);

/// `$redis->psetex(string $key, int $expire, mixed $value)`
pub const PSETEX: MethodEntry = MethodEntry::public_async(
    "psetex",
    &[
        Param::new("key", ParamType::String),
        Param::new("expire", ParamType::Int),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("psetex", c, o, a),
);

/// `$redis->pttl(string $key)`
pub const PTTL: MethodEntry = MethodEntry::public_async(
    "pttl",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("pttl", c, o, a),
);

/// `$redis->randomKey()`
pub const RANDOM_KEY: MethodEntry =
    MethodEntry::public_async("randomKey", &[], |c, o, a| dispatch("randomkey", c, o, a));

/// `$redis->rename(string $old_name, string $new_name)`
pub const RENAME: MethodEntry = MethodEntry::public_async(
    "rename",
    &[
        Param::new("old_name", ParamType::String),
        Param::new("new_name", ParamType::String),
    ],
    |c, o, a| dispatch("rename", c, o, a),
);

/// `$redis->renameNx(string $key_src, string $key_dst)`
pub const RENAME_NX: MethodEntry = MethodEntry::public_async(
    "renameNx",
    &[
        Param::new("key_src", ParamType::String),
        Param::new("key_dst", ParamType::String),
    ],
    |c, o, a| dispatch("renamenx", c, o, a),
);

/// `$redis->restore(string $key, int $ttl, string $value, ?array $options = null)`
pub const RESTORE: MethodEntry = MethodEntry::public_async(
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
);

/// `$redis->set(string $key, mixed $value, mixed $options = null)`
pub const SET: MethodEntry = MethodEntry::public_async(
    "set",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
        Param::new("options", ParamType::Mixed).optional(),
    ],
    |c, o, a| dispatch("set", c, o, a),
);

/// `$redis->setBit(string $key, int $idx, bool $value)`
pub const SET_BIT: MethodEntry = MethodEntry::public_async(
    "setBit",
    &[
        Param::new("key", ParamType::String),
        Param::new("idx", ParamType::Int),
        Param::new("value", ParamType::Bool),
    ],
    |c, o, a| dispatch("setbit", c, o, a),
);

/// `$redis->setRange(string $key, int $index, string $value)`
pub const SET_RANGE: MethodEntry = MethodEntry::public_async(
    "setRange",
    &[
        Param::new("key", ParamType::String),
        Param::new("index", ParamType::Int),
        Param::new("value", ParamType::String),
    ],
    |c, o, a| dispatch("setrange", c, o, a),
);

/// `$redis->setex(string $key, int $expire, mixed $value)`
pub const SETEX: MethodEntry = MethodEntry::public_async(
    "setex",
    &[
        Param::new("key", ParamType::String),
        Param::new("expire", ParamType::Int),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("setex", c, o, a),
);

/// `$redis->setnx(string $key, mixed $value)`
pub const SETNX: MethodEntry = MethodEntry::public_async(
    "setnx",
    &[
        Param::new("key", ParamType::String),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("setnx", c, o, a),
);

/// `$redis->touch(array|string $key_or_array, string ...$more_keys)`
pub const TOUCH: MethodEntry = MethodEntry::public_async(
    "touch",
    &[
        Param::new("key_or_array", ParamType::ArrayOrString),
        Param::new("more_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("touch", c, o, a),
)
.with_preparation(|c, _, a| arguments::key_list("Redis::touch", TOUCH_KEYS, c, a));
/// How phpredis parses `touch()`: any value is a key.
const TOUCH_KEYS: &[Param] = &[
    Param::new("key_or_array", ParamType::Mixed),
    Param::new("more_keys", ParamType::Mixed).variadic(),
];

/// `$redis->strlen(string $key)`
pub const STRLEN: MethodEntry = MethodEntry::public_async(
    "strlen",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("strlen", c, o, a),
);

/// `$redis->ttl(string $key)`
pub const TTL: MethodEntry =
    MethodEntry::public_async("ttl", &[Param::new("key", ParamType::String)], |c, o, a| {
        dispatch("ttl", c, o, a)
    });

/// `$redis->type(string $key)`
pub const TYPE: MethodEntry = MethodEntry::public_async(
    "type",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("type", c, o, a),
);

/// `$redis->unlink(array|string $key, string ...$other_keys)`
pub const UNLINK: MethodEntry = MethodEntry::public_async(
    "unlink",
    &[
        Param::new("key", ParamType::ArrayOrString),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("unlink", c, o, a),
)
.with_preparation(|c, _, a| arguments::key_list("Redis::unlink", UNLINK_KEYS, c, a));
/// How phpredis parses `unlink()`: any value is a key.
const UNLINK_KEYS: &[Param] = &[
    Param::new("key", ParamType::Mixed),
    Param::new("other_keys", ParamType::Mixed).variadic(),
];
