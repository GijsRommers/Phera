//! Publish and subscribe: the PHP signatures, implemented in `crate::pubsub`.

use phera_extension_api::{MethodEntry, Param, ParamType};

use crate::dispatch::dispatch;

/// `$redis->psubscribe(array $patterns, callable $cb)`
pub const PSUBSCRIBE: MethodEntry = MethodEntry::public_async(
    "psubscribe",
    &[
        Param::new("patterns", ParamType::Array),
        Param::new("cb", ParamType::Callable),
    ],
    |c, o, a| dispatch("psubscribe", c, o, a),
);

/// `$redis->publish(string $channel, string $message)`
pub const PUBLISH: MethodEntry = MethodEntry::public_async(
    "publish",
    &[
        Param::new("channel", ParamType::String),
        Param::new("message", ParamType::String),
    ],
    |c, o, a| dispatch("publish", c, o, a),
);

/// `$redis->pubsub(string $command, mixed $arg = null)`
pub const PUBSUB: MethodEntry = MethodEntry::public_async(
    "pubsub",
    &[
        Param::new("command", ParamType::String),
        Param::new("arg", ParamType::Mixed).optional(),
    ],
    |c, o, a| dispatch("pubsub", c, o, a),
);

/// `$redis->punsubscribe(array $patterns)`
pub const PUNSUBSCRIBE: MethodEntry = MethodEntry::public_async(
    "punsubscribe",
    &[Param::new("patterns", ParamType::Array)],
    |c, o, a| dispatch("punsubscribe", c, o, a),
);

/// `$redis->ssubscribe(array $channels, callable $cb)`
pub const SSUBSCRIBE: MethodEntry = MethodEntry::public_async(
    "ssubscribe",
    &[
        Param::new("channels", ParamType::Array),
        Param::new("cb", ParamType::Callable),
    ],
    |c, o, a| dispatch("ssubscribe", c, o, a),
);

/// `$redis->subscribe(array $channels, callable $cb)`
pub const SUBSCRIBE: MethodEntry = MethodEntry::public_async(
    "subscribe",
    &[
        Param::new("channels", ParamType::Array),
        Param::new("cb", ParamType::Callable),
    ],
    |c, o, a| dispatch("subscribe", c, o, a),
);

/// `$redis->sunsubscribe(array $channels)`
pub const SUNSUBSCRIBE: MethodEntry = MethodEntry::public_async(
    "sunsubscribe",
    &[Param::new("channels", ParamType::Array)],
    |c, o, a| dispatch("sunsubscribe", c, o, a),
);

/// `$redis->unsubscribe(array $channels)`
pub const UNSUBSCRIBE: MethodEntry = MethodEntry::public_async(
    "unsubscribe",
    &[Param::new("channels", ParamType::Array)],
    |c, o, a| dispatch("unsubscribe", c, o, a),
);
