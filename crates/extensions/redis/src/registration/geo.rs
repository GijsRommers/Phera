//! Geospatial indexes: the PHP signatures, built in `crate::commands::geo`.

use phera_extension_api::{MethodEntry, Param, ParamType};

use crate::arguments;
use crate::dispatch::dispatch;

/// `$redis->geoadd(string $key, float $lng, float $lat, string $member, mixed ...$other_triples_and_options)`
pub const GEOADD: MethodEntry = MethodEntry::public_async(
    "geoadd",
    &[
        Param::new("key", ParamType::String),
        Param::new("lng", ParamType::Float),
        Param::new("lat", ParamType::Float),
        Param::new("member", ParamType::String),
        Param::new("other_triples_and_options", ParamType::Mixed)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("geoadd", c, o, a),
)
.with_preparation(|c, _, a| {
    arguments::not_empty("Redis::geoadd", a)?;
    arguments::key_list("Redis::geoadd", GEOADD_KEYS, c, a)
});
/// How phpredis parses `geoadd()`: any value is a key.
const GEOADD_KEYS: &[Param] = &[
    Param::new("key", ParamType::Mixed),
    Param::new("lng", ParamType::Mixed),
    Param::new("lat", ParamType::Mixed),
    Param::new("member", ParamType::Mixed),
    Param::new("other_triples_and_options", ParamType::Mixed).variadic(),
];

/// `$redis->geodist(string $key, string $src, string $dst, ?string $unit = null)`
pub const GEODIST: MethodEntry = MethodEntry::public_async(
    "geodist",
    &[
        Param::new("key", ParamType::String),
        Param::new("src", ParamType::String),
        Param::new("dst", ParamType::String),
        Param::new("unit", ParamType::String).nullable().optional(),
    ],
    |c, o, a| dispatch("geodist", c, o, a),
);

/// `$redis->geohash(string $key, string $member, string ...$other_members)`
pub const GEOHASH: MethodEntry = MethodEntry::public_async(
    "geohash",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::String),
        Param::new("other_members", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("geohash", c, o, a),
);

/// `$redis->geopos(string $key, string $member, string ...$other_members)`
pub const GEOPOS: MethodEntry = MethodEntry::public_async(
    "geopos",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::String),
        Param::new("other_members", ParamType::String)
            .optional()
            .variadic(),
    ],
    |c, o, a| dispatch("geopos", c, o, a),
);

/// `$redis->georadius(string $key, float $lng, float $lat, float $radius, string $unit, array $options = [])`
pub const GEORADIUS: MethodEntry = MethodEntry::public_async(
    "georadius",
    &[
        Param::new("key", ParamType::String),
        Param::new("lng", ParamType::Float),
        Param::new("lat", ParamType::Float),
        Param::new("radius", ParamType::Float),
        Param::new("unit", ParamType::String),
        Param::new("options", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("georadius", c, o, a),
);

/// `$redis->georadius_ro(string $key, float $lng, float $lat, float $radius, string $unit, array $options = [])`
pub const GEORADIUS_RO: MethodEntry = MethodEntry::public_async(
    "georadius_ro",
    &[
        Param::new("key", ParamType::String),
        Param::new("lng", ParamType::Float),
        Param::new("lat", ParamType::Float),
        Param::new("radius", ParamType::Float),
        Param::new("unit", ParamType::String),
        Param::new("options", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("georadius_ro", c, o, a),
);

/// `$redis->georadiusbymember(string $key, string $member, float $radius, string $unit, array $options = [])`
pub const GEORADIUSBYMEMBER: MethodEntry = MethodEntry::public_async(
    "georadiusbymember",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::String),
        Param::new("radius", ParamType::Float),
        Param::new("unit", ParamType::String),
        Param::new("options", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("georadiusbymember", c, o, a),
);

/// `$redis->georadiusbymember_ro(string $key, string $member, float $radius, string $unit, array $options = [])`
pub const GEORADIUSBYMEMBER_RO: MethodEntry = MethodEntry::public_async(
    "georadiusbymember_ro",
    &[
        Param::new("key", ParamType::String),
        Param::new("member", ParamType::String),
        Param::new("radius", ParamType::Float),
        Param::new("unit", ParamType::String),
        Param::new("options", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("georadiusbymember_ro", c, o, a),
);

/// `$redis->geosearch(string $key, array|string $position, array|int|float $shape, string $unit, array $options = [])`
pub const GEOSEARCH: MethodEntry = MethodEntry::public_async(
    "geosearch",
    &[
        Param::new("key", ParamType::String),
        Param::new("position", ParamType::ArrayOrString),
        Param::new("shape", ParamType::ArrayOrNumber),
        Param::new("unit", ParamType::String),
        Param::new("options", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("geosearch", c, o, a),
);

/// `$redis->geosearchstore(string $dst, string $src, array|string $position, array|int|float $shape, string $unit, array $options = [])`
pub const GEOSEARCHSTORE: MethodEntry = MethodEntry::public_async(
    "geosearchstore",
    &[
        Param::new("dst", ParamType::String),
        Param::new("src", ParamType::String),
        Param::new("position", ParamType::ArrayOrString),
        Param::new("shape", ParamType::ArrayOrNumber),
        Param::new("unit", ParamType::String),
        Param::new("options", ParamType::Array).optional(),
    ],
    |c, o, a| dispatch("geosearchstore", c, o, a),
);
