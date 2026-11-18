//! SCAN and its HSCAN, SSCAN, ZSCAN variants: the PHP signatures, implemented in `crate::scan`.

use phera_extension_api::{MethodEntry, Param, ParamType};

use crate::dispatch::dispatch;

/// `$redis->hscan(string $key, string|int|null &$iterator, ?string $pattern = null, int $count = 0)`
pub const HSCAN: MethodEntry = MethodEntry::public_async(
    "hscan",
    &[
        Param::new("key", ParamType::String),
        Param::new("iterator", ParamType::IntOrString)
            .nullable()
            .by_ref(),
        Param::new("pattern", ParamType::String)
            .nullable()
            .optional(),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("hscan", c, o, a),
);

/// `$redis->scan(string|int|null &$iterator, ?string $pattern = null, int $count = 0, ?string $type = null)`
pub const SCAN: MethodEntry = MethodEntry::public_async(
    "scan",
    &[
        Param::new("iterator", ParamType::IntOrString)
            .nullable()
            .by_ref(),
        Param::new("pattern", ParamType::String)
            .nullable()
            .optional(),
        Param::new("count", ParamType::Int).optional(),
        Param::new("type", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("scan", c, o, a),
);

/// `$redis->sscan(string $key, string|int|null &$iterator, ?string $pattern = null, int $count = 0)`
pub const SSCAN: MethodEntry = MethodEntry::public_async(
    "sscan",
    &[
        Param::new("key", ParamType::String),
        Param::new("iterator", ParamType::IntOrString)
            .nullable()
            .by_ref(),
        Param::new("pattern", ParamType::String)
            .nullable()
            .optional(),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("sscan", c, o, a),
);

/// `$redis->zscan(string $key, string|int|null &$iterator, ?string $pattern = null, int $count = 0)`
pub const ZSCAN: MethodEntry = MethodEntry::public_async(
    "zscan",
    &[
        Param::new("key", ParamType::String),
        Param::new("iterator", ParamType::IntOrString)
            .nullable()
            .by_ref(),
        Param::new("pattern", ParamType::String)
            .nullable()
            .optional(),
        Param::new("count", ParamType::Int).optional(),
    ],
    |c, o, a| dispatch("zscan", c, o, a),
);
