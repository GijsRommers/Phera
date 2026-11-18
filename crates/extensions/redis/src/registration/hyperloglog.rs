//! HyperLogLog: the PHP signatures, built in `crate::commands::collections`.

use phera_extension_api::{MethodEntry, Param, ParamType};

use crate::arguments;
use crate::dispatch::dispatch;

/// `$redis->pfadd(string $key, array $elements)`
pub const PFADD: MethodEntry = MethodEntry::public_async(
    "pfadd",
    &[
        Param::new("key", ParamType::String),
        Param::new("elements", ParamType::Array),
    ],
    |c, o, a| dispatch("pfadd", c, o, a),
);

/// `$redis->pfcount(array|string $key_or_keys)`
pub const PFCOUNT: MethodEntry = MethodEntry::public_async(
    "pfcount",
    &[Param::new("key_or_keys", ParamType::ArrayOrString)],
    |c, o, a| dispatch("pfcount", c, o, a),
)
.with_preparation(|c, _, a| arguments::pfcount("Redis::pfcount", c, a));

/// `$redis->pfmerge(string $dst, array $srckeys)`
pub const PFMERGE: MethodEntry = MethodEntry::public_async(
    "pfmerge",
    &[
        Param::new("dst", ParamType::String),
        Param::new("srckeys", ParamType::Array),
    ],
    |c, o, a| dispatch("pfmerge", c, o, a),
);
