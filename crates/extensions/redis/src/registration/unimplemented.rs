//! Methods that are declared but not implemented: the PHP signatures, each throws a RedisException; see the README.

use phera_extension_api::{MethodEntry, Param, ParamType};

use super::unimplemented;

/// `$redis->failover(?array $to = null, bool $abort = false, int $timeout = 0)`
pub const FAILOVER: MethodEntry = MethodEntry::public_async(
    "failover",
    &[
        Param::new("to", ParamType::Array).nullable().optional(),
        Param::new("abort", ParamType::Bool).optional(),
        Param::new("timeout", ParamType::Int).optional(),
    ],
    |_, _, _| unimplemented("failover"),
);

/// `$redis->getTransferredBytes()`
pub const GET_TRANSFERRED_BYTES: MethodEntry =
    MethodEntry::public_async("getTransferredBytes", &[], |_, _, _| {
        unimplemented("getTransferredBytes")
    });

/// `$redis->clearTransferredBytes()`
pub const CLEAR_TRANSFERRED_BYTES: MethodEntry =
    MethodEntry::public_async("clearTransferredBytes", &[], |_, _, _| {
        unimplemented("clearTransferredBytes")
    });

/// `$redis->migrate(string $host, int $port, array|string $key, int $dstdb, int $timeout, bool $copy = false, bool $replace = false, mixed $credentials = null)`
pub const MIGRATE: MethodEntry = MethodEntry::public_async(
    "migrate",
    &[
        Param::new("host", ParamType::String),
        Param::new("port", ParamType::Int),
        Param::new("key", ParamType::ArrayOrString),
        Param::new("dstdb", ParamType::Int),
        Param::new("timeout", ParamType::Int),
        Param::new("copy", ParamType::Bool).optional(),
        Param::new("replace", ParamType::Bool).optional(),
        Param::new("credentials", ParamType::Mixed).optional(),
    ],
    |_, _, _| unimplemented("migrate"),
);

/// `$redis->vadd(string $key, array $values, mixed $element, ?array $options = null)`
pub const VADD: MethodEntry = MethodEntry::public_async(
    "vadd",
    &[
        Param::new("key", ParamType::String),
        Param::new("values", ParamType::Array),
        Param::new("element", ParamType::Mixed),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |_, _, _| unimplemented("vadd"),
);

/// `$redis->vsim(string $key, mixed $member, ?array $options = null)`
pub const VSIM: MethodEntry = MethodEntry::public_async(
    "vsim",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |_, _, _| unimplemented("vsim"),
);

/// `$redis->vcard(string $key)`
pub const VCARD: MethodEntry = MethodEntry::public_async(
    "vcard",
    &[Param::new("key", ParamType::String)],
    |_, _, _| unimplemented("vcard"),
);

/// `$redis->vdim(string $key)`
pub const VDIM: MethodEntry = MethodEntry::public_async(
    "vdim",
    &[Param::new("key", ParamType::String)],
    |_, _, _| unimplemented("vdim"),
);

/// `$redis->vinfo(string $key)`
pub const VINFO: MethodEntry = MethodEntry::public_async(
    "vinfo",
    &[Param::new("key", ParamType::String)],
    |_, _, _| unimplemented("vinfo"),
);

/// `$redis->vismember(string $key, mixed $member)`
pub const VISMEMBER: MethodEntry = MethodEntry::public_async(
    "vismember",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
    ],
    |_, _, _| unimplemented("vismember"),
);

/// `$redis->vemb(string $key, mixed $member, bool $raw = false)`
pub const VEMB: MethodEntry = MethodEntry::public_async(
    "vemb",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
        Param::new("raw", ParamType::Bool).optional(),
    ],
    |_, _, _| unimplemented("vemb"),
);

/// `$redis->vrandmember(string $key, int $count = 0)`
pub const VRANDMEMBER: MethodEntry = MethodEntry::public_async(
    "vrandmember",
    &[
        Param::new("key", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |_, _, _| unimplemented("vrandmember"),
);

/// `$redis->vrange(string $key, string $min, string $max, int $count = -1)`
pub const VRANGE: MethodEntry = MethodEntry::public_async(
    "vrange",
    &[
        Param::new("key", ParamType::String),
        Param::new("min", ParamType::String),
        Param::new("max", ParamType::String),
        Param::new("count", ParamType::Int).optional(),
    ],
    |_, _, _| unimplemented("vrange"),
);

/// `$redis->vrem(string $key, mixed $member)`
pub const VREM: MethodEntry = MethodEntry::public_async(
    "vrem",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
    ],
    |_, _, _| unimplemented("vrem"),
);

/// `$redis->vsetattr(string $key, mixed $member, array|string $attributes)`
pub const VSETATTR: MethodEntry = MethodEntry::public_async(
    "vsetattr",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
        Param::new("attributes", ParamType::ArrayOrString),
    ],
    |_, _, _| unimplemented("vsetattr"),
);

/// `$redis->vgetattr(string $key, mixed $member, bool $decode = true)`
pub const VGETATTR: MethodEntry = MethodEntry::public_async(
    "vgetattr",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
        Param::new("decode", ParamType::Bool).optional(),
    ],
    |_, _, _| unimplemented("vgetattr"),
);

/// `$redis->vlinks(string $key, mixed $member, bool $withscores = false)`
pub const VLINKS: MethodEntry = MethodEntry::public_async(
    "vlinks",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::Mixed),
        Param::new("withscores", ParamType::Bool).optional(),
    ],
    |_, _, _| unimplemented("vlinks"),
);
