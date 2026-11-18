//! Server, scripting and functions: the PHP signatures, built in `crate::commands::server`.

use phera_extension_api::{MethodEntry, Param, ParamType};

use crate::arguments;
use crate::dispatch::dispatch;

/// `$redis->acl(string $subcmd, string ...$args)`
pub const ACL: MethodEntry = MethodEntry::public_async(
    "acl",
    &[
        Param::new("subcmd", ParamType::String),
        Param::new("args", ParamType::String).optional().variadic(),
    ],
    |c, o, a| dispatch("acl", c, o, a),
);

/// `$redis->bgSave()`
pub const BG_SAVE: MethodEntry =
    MethodEntry::public_async("bgSave", &[], |c, o, a| dispatch("bgsave", c, o, a));

/// `$redis->bgrewriteaof()`
pub const BGREWRITEAOF: MethodEntry = MethodEntry::public_async("bgrewriteaof", &[], |c, o, a| {
    dispatch("bgrewriteaof", c, o, a)
});

/// `$redis->waitaof(int $numlocal, int $numreplicas, int $timeout)`
pub const WAITAOF: MethodEntry = MethodEntry::public_async(
    "waitaof",
    &[
        Param::new("numlocal", ParamType::Int),
        Param::new("numreplicas", ParamType::Int),
        Param::new("timeout", ParamType::Int),
    ],
    |c, o, a| dispatch("waitaof", c, o, a),
);

/// `$redis->client(string $opt, mixed ...$args)`
pub const CLIENT: MethodEntry = MethodEntry::public_async(
    "client",
    &[
        Param::new("opt", ParamType::String),
        Param::new("args", ParamType::Mixed).optional().variadic(),
    ],
    |c, o, a| dispatch("client", c, o, a),
);

/// `$redis->command(?string $opt = null, mixed ...$args)`
pub const COMMAND: MethodEntry = MethodEntry::public_async(
    "command",
    &[
        Param::new("opt", ParamType::String).nullable().optional(),
        Param::new("args", ParamType::Mixed).optional().variadic(),
    ],
    |c, o, a| dispatch("command", c, o, a),
)
.with_preparation(|c, _, a| arguments::object_not_null("Redis::command", COMMAND_PARSED, c, a));
/// How phpredis parses `command()` when it is given an object.
const COMMAND_PARSED: &[Param] = &[
    Param::new("opt", ParamType::String).optional(),
    Param::new("args", ParamType::Mixed).variadic(),
];

/// `$redis->config(string $operation, array|string|null $key_or_settings = null, ?string $value = null)`
pub const CONFIG: MethodEntry = MethodEntry::public_async(
    "config",
    &[
        Param::new("operation", ParamType::String),
        Param::new("key_or_settings", ParamType::ArrayOrString)
            .nullable()
            .optional(),
        Param::new("value", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("config", c, o, a),
);

/// `$redis->dbSize()`
pub const DB_SIZE: MethodEntry =
    MethodEntry::public_async("dbSize", &[], |c, o, a| dispatch("dbsize", c, o, a));

/// `$redis->debug(string $key)`
pub const DEBUG: MethodEntry = MethodEntry::public_async(
    "debug",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("debug", c, o, a),
);

/// `$redis->echo(string $str)`
pub const ECHO: MethodEntry = MethodEntry::public_async(
    "echo",
    &[Param::new("str", ParamType::String)],
    |c, o, a| dispatch("echo", c, o, a),
);

/// `$redis->eval(string $script, array $args = [], int $num_keys = 0)`
pub const EVAL: MethodEntry = MethodEntry::public_async(
    "eval",
    &[
        Param::new("script", ParamType::String),
        Param::new("args", ParamType::Array).optional(),
        Param::new("num_keys", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("eval", c, o, a),
);

/// `$redis->eval_ro(string $script_sha, array $args = [], int $num_keys = 0)`
pub const EVAL_RO: MethodEntry = MethodEntry::public_async(
    "eval_ro",
    &[
        Param::new("script_sha", ParamType::String),
        Param::new("args", ParamType::Array).optional(),
        Param::new("num_keys", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("eval_ro", c, o, a),
);

/// `$redis->evalsha(string $sha1, array $args = [], int $num_keys = 0)`
pub const EVALSHA: MethodEntry = MethodEntry::public_async(
    "evalsha",
    &[
        Param::new("sha1", ParamType::String),
        Param::new("args", ParamType::Array).optional(),
        Param::new("num_keys", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("evalsha", c, o, a),
);

/// `$redis->evalsha_ro(string $sha1, array $args = [], int $num_keys = 0)`
pub const EVALSHA_RO: MethodEntry = MethodEntry::public_async(
    "evalsha_ro",
    &[
        Param::new("sha1", ParamType::String),
        Param::new("args", ParamType::Array).optional(),
        Param::new("num_keys", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("evalsha_ro", c, o, a),
);

/// `$redis->fcall(string $fn, array $keys = [], array $args = [])`
pub const FCALL: MethodEntry = MethodEntry::public_async(
    "fcall",
    &[
        Param::new("fn", ParamType::String),
        Param::new("keys", ParamType::Array).optional(),
        Param::new("args", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("fcall", c, o, a),
);

/// `$redis->fcall_ro(string $fn, array $keys = [], array $args = [])`
pub const FCALL_RO: MethodEntry = MethodEntry::public_async(
    "fcall_ro",
    &[
        Param::new("fn", ParamType::String),
        Param::new("keys", ParamType::Array).optional(),
        Param::new("args", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("fcall_ro", c, o, a),
);

/// `$redis->flushAll(?bool $sync = null)`
pub const FLUSH_ALL: MethodEntry = MethodEntry::public_async(
    "flushAll",
    &[Param::new("sync", ParamType::Bool).nullable().optional()],
    |c, o, a| dispatch("flushall", c, o, a),
);

/// `$redis->flushDB(?bool $sync = null)`
pub const FLUSH_DB: MethodEntry = MethodEntry::public_async(
    "flushDB",
    &[Param::new("sync", ParamType::Bool).nullable().optional()],
    |c, o, a| dispatch("flushdb", c, o, a),
);

/// `$redis->function(string $operation, mixed ...$args)`
pub const FUNCTION: MethodEntry = MethodEntry::public_async(
    "function",
    &[
        Param::new("operation", ParamType::String),
        Param::new("args", ParamType::Mixed).optional().variadic(),
    ],
    |c, o, a| dispatch("function", c, o, a),
);

/// `$redis->info(string ...$sections)`
pub const INFO: MethodEntry = MethodEntry::public_async(
    "info",
    &[Param::new("sections", ParamType::String)
        .optional()
        .variadic()],
    |c, o, a| dispatch("info", c, o, a),
);

/// `$redis->lastSave()`
pub const LAST_SAVE: MethodEntry =
    MethodEntry::public_async("lastSave", &[], |c, o, a| dispatch("lastsave", c, o, a));

/// `$redis->move(string $key, int $index)`
pub const MOVE: MethodEntry = MethodEntry::public_async(
    "move",
    &[
        Param::new("key", ParamType::String),
        Param::new("index", ParamType::Int),
    ],
    |c, o, a| dispatch("move", c, o, a),
);

/// `$redis->object(string $subcommand, string $key)`
pub const OBJECT: MethodEntry = MethodEntry::public_async(
    "object",
    &[
        Param::new("subcommand", ParamType::String),
        Param::new("key", ParamType::String),
    ],
    |c, o, a| dispatch("object", c, o, a),
);

/// `$redis->ping(?string $message = null)`
pub const PING: MethodEntry = MethodEntry::public_async(
    "ping",
    &[Param::new("message", ParamType::String)
        .nullable()
        .optional()],
    |c, o, a| dispatch("ping", c, o, a),
);

/// `$redis->rawcommand(string $command, mixed ...$args)`
pub const RAWCOMMAND: MethodEntry = MethodEntry::public_async(
    "rawcommand",
    &[
        Param::new("command", ParamType::String),
        Param::new("args", ParamType::Mixed).optional().variadic(),
    ],
    |c, o, a| dispatch("rawcommand", c, o, a),
)
.with_preparation(|c, _, a| arguments::raw_command("Redis::rawcommand", c, a));

/// `$redis->reset()`
pub const RESET: MethodEntry =
    MethodEntry::public_async("reset", &[], |c, o, a| dispatch("reset", c, o, a));

/// `$redis->role()`
pub const ROLE: MethodEntry =
    MethodEntry::public_async("role", &[], |c, o, a| dispatch("role", c, o, a));

/// `$redis->save()`
pub const SAVE: MethodEntry =
    MethodEntry::public_async("save", &[], |c, o, a| dispatch("save", c, o, a));

/// `$redis->script(string $command, mixed ...$args)`
pub const SCRIPT: MethodEntry = MethodEntry::public_async(
    "script",
    &[
        Param::new("command", ParamType::String),
        Param::new("args", ParamType::Mixed).optional().variadic(),
    ],
    |c, o, a| dispatch("script", c, o, a),
)
.with_preparation(|_, _, a| arguments::script(a));

/// `$redis->replicaof(?string $host = null, int $port = 6379)`
pub const REPLICAOF: MethodEntry = MethodEntry::public_async(
    "replicaof",
    &[
        Param::new("host", ParamType::String).nullable().optional(),
        Param::new("port", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("replicaof", c, o, a),
)
.with_preparation(|c, _, a| arguments::object_not_null("Redis::replicaof", REPLICAOF_PARSED, c, a));
/// How phpredis parses `replicaof()` when it is given an object.
const REPLICAOF_PARSED: &[Param] = &[
    Param::new("host", ParamType::String).optional(),
    Param::new("port", ParamType::Int).optional(),
];

/// `$redis->slowlog(string $operation, int $length = 0)`
pub const SLOWLOG: MethodEntry = MethodEntry::public_async(
    "slowlog",
    &[
        Param::new("operation", ParamType::String),
        Param::new("length", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("slowlog", c, o, a),
);

/// `$redis->swapdb(int $src, int $dst)`
pub const SWAPDB: MethodEntry = MethodEntry::public_async(
    "swapdb",
    &[
        Param::new("src", ParamType::Int),
        Param::new("dst", ParamType::Int),
    ],
    |c, o, a| dispatch("swapdb", c, o, a),
);

/// `$redis->time()`
pub const TIME: MethodEntry =
    MethodEntry::public_async("time", &[], |c, o, a| dispatch("time", c, o, a));

/// `$redis->wait(int $numreplicas, int $timeout)`
pub const WAIT: MethodEntry = MethodEntry::public_async(
    "wait",
    &[
        Param::new("numreplicas", ParamType::Int),
        Param::new("timeout", ParamType::Int),
    ],
    |c, o, a| dispatch("wait", c, o, a),
);
