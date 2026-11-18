//! MULTI, pipelines and WATCH: the PHP signatures, implemented in `crate::transaction`.

use phera_extension_api::{MethodEntry, Param, ParamType};

use crate::arguments;
use crate::dispatch::dispatch;

/// `$redis->discard()`
pub const DISCARD: MethodEntry =
    MethodEntry::public_async("discard", &[], |c, o, a| dispatch("discard", c, o, a));

/// `$redis->exec()`
pub const EXEC: MethodEntry =
    MethodEntry::public_async("exec", &[], |c, o, a| dispatch("exec", c, o, a));

/// `$redis->multi(int $value = 1)`
pub const MULTI: MethodEntry = MethodEntry::public_async(
    "multi",
    &[Param::new("value", ParamType::Int).optional()],
    |c, o, a| dispatch("multi", c, o, a),
);

/// `$redis->pipeline()`
pub const PIPELINE: MethodEntry =
    MethodEntry::public_async("pipeline", &[], |c, o, a| dispatch("pipeline", c, o, a));

/// `$redis->unwatch()`
pub const UNWATCH: MethodEntry =
    MethodEntry::public_async("unwatch", &[], |c, o, a| dispatch("unwatch", c, o, a));

/// `$redis->watch(array|string $key, string ...$other_keys)`
pub const WATCH: MethodEntry = MethodEntry::public_async(
    "watch",
    &[
        Param::new("key", ParamType::ArrayOrString),
        Param::new("other_keys", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("watch", c, o, a),
)
.with_preparation(|c, _, a| arguments::key_list("Redis::watch", WATCH_KEYS, c, a));
/// How phpredis parses `watch()`: any value is a key.
const WATCH_KEYS: &[Param] = &[
    Param::new("key", ParamType::Mixed),
    Param::new("other_keys", ParamType::Mixed).variadic(),
];
