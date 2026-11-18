//! SORT and SORT_RO: the PHP signatures, built in `crate::commands::sort`.

use phera_extension_api::{MethodEntry, Param, ParamType};

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
