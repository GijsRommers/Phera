//! SORT and its deprecated shortcuts: the PHP signatures, built in `crate::commands::sort`.

use phera_extension_api::{MethodEntry, MethodPreparation, Param, ParamType};

use crate::arguments;
use crate::dispatch::dispatch;

/// `$redis->sort(string $key, ?array $options = null)`
pub const SORT: MethodEntry = MethodEntry::public_async(
    "sort",
    &[
        Param::new("key", ParamType::String),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("sort", c, o, a),
)
.with_preparation(|c, _, a| arguments::parse_as("Redis::sort", SORT_PARSED, c, a));
/// How phpredis parses `sort()`, which differs from its declaration.
const SORT_PARSED: &[Param] = &[
    Param::new("key", ParamType::String),
    Param::new("options", ParamType::Array).optional(),
];

/// `$redis->sort_ro(string $key, ?array $options = null)`
pub const SORT_RO: MethodEntry = MethodEntry::public_async(
    "sort_ro",
    &[
        Param::new("key", ParamType::String),
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("sort_ro", c, o, a),
)
.with_preparation(|c, _, a| arguments::parse_as("Redis::sort_ro", SORT_RO_PARSED, c, a));
/// How phpredis parses `sort_ro()`, which differs from its declaration.
const SORT_RO_PARSED: &[Param] = &[
    Param::new("key", ParamType::String),
    Param::new("options", ParamType::Array).optional(),
];

/// `$redis->sortAsc(string $key, ?string $pattern = null, mixed $get = null, int $offset = -1, int $count = -1, ?string $store = null)`
pub const SORT_ASC: MethodEntry = MethodEntry::public_async(
    "sortAsc",
    &[
        Param::new("key", ParamType::String),
        Param::new("pattern", ParamType::String)
            .nullable()
            .optional(),
        Param::new("get", ParamType::Mixed).optional(),
        Param::new("offset", ParamType::Int).optional(),
        Param::new("count", ParamType::Int).optional(),
        Param::new("store", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("sortasc", c, o, a),
)
.with_preparation(|c, _, _| {
    arguments::deprecated("Redis::sortAsc", c);
    Ok(MethodPreparation::Default)
});

/// `$redis->sortAscAlpha(string $key, ?string $pattern = null, mixed $get = null, int $offset = -1, int $count = -1, ?string $store = null)`
pub const SORT_ASC_ALPHA: MethodEntry = MethodEntry::public_async(
    "sortAscAlpha",
    &[
        Param::new("key", ParamType::String),
        Param::new("pattern", ParamType::String)
            .nullable()
            .optional(),
        Param::new("get", ParamType::Mixed).optional(),
        Param::new("offset", ParamType::Int).optional(),
        Param::new("count", ParamType::Int).optional(),
        Param::new("store", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("sortascalpha", c, o, a),
)
.with_preparation(|c, _, _| {
    arguments::deprecated("Redis::sortAscAlpha", c);
    Ok(MethodPreparation::Default)
});

/// `$redis->sortDesc(string $key, ?string $pattern = null, mixed $get = null, int $offset = -1, int $count = -1, ?string $store = null)`
pub const SORT_DESC: MethodEntry = MethodEntry::public_async(
    "sortDesc",
    &[
        Param::new("key", ParamType::String),
        Param::new("pattern", ParamType::String)
            .nullable()
            .optional(),
        Param::new("get", ParamType::Mixed).optional(),
        Param::new("offset", ParamType::Int).optional(),
        Param::new("count", ParamType::Int).optional(),
        Param::new("store", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("sortdesc", c, o, a),
)
.with_preparation(|c, _, _| {
    arguments::deprecated("Redis::sortDesc", c);
    Ok(MethodPreparation::Default)
});

/// `$redis->sortDescAlpha(string $key, ?string $pattern = null, mixed $get = null, int $offset = -1, int $count = -1, ?string $store = null)`
pub const SORT_DESC_ALPHA: MethodEntry = MethodEntry::public_async(
    "sortDescAlpha",
    &[
        Param::new("key", ParamType::String),
        Param::new("pattern", ParamType::String)
            .nullable()
            .optional(),
        Param::new("get", ParamType::Mixed).optional(),
        Param::new("offset", ParamType::Int).optional(),
        Param::new("count", ParamType::Int).optional(),
        Param::new("store", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("sortdescalpha", c, o, a),
)
.with_preparation(|c, _, _| {
    arguments::deprecated("Redis::sortDescAlpha", c);
    Ok(MethodPreparation::Default)
});
