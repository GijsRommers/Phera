//! Connecting, options and local state: the PHP signatures, implemented in `crate::dispatch` and `crate::connection`.

use phera_extension_api::{MethodEntry, MethodPreparation, Param, ParamType};

use crate::arguments;
use crate::dispatch::dispatch;

/// `$redis->__construct(?array $options = null)`
pub const CONSTRUCT: MethodEntry = MethodEntry::public_async(
    "__construct",
    &[Param::new("options", ParamType::Array)
        .nullable()
        .optional()],
    |c, o, a| dispatch("__construct", c, o, a),
);

/// `$redis->__destruct()`
pub const DESTRUCT: MethodEntry =
    MethodEntry::public_async("__destruct", &[], |c, o, a| dispatch("__destruct", c, o, a));

/// `$redis->_compress(string $value)`
pub const COMPRESS: MethodEntry = MethodEntry::public_async(
    "_compress",
    &[Param::new("value", ParamType::String)],
    |c, o, a| dispatch("_compress", c, o, a),
);

/// `$redis->_uncompress(string $value)`
pub const UNCOMPRESS: MethodEntry = MethodEntry::public_async(
    "_uncompress",
    &[Param::new("value", ParamType::String)],
    |c, o, a| dispatch("_uncompress", c, o, a),
);

/// `$redis->_prefix(string $key)`
pub const PREFIX: MethodEntry = MethodEntry::public_async(
    "_prefix",
    &[Param::new("key", ParamType::String)],
    |c, o, a| dispatch("_prefix", c, o, a),
);

/// `$redis->_serialize(mixed $value)`
pub const SERIALIZE: MethodEntry = MethodEntry::public_async(
    "_serialize",
    &[Param::new("value", ParamType::Mixed)],
    |c, o, a| dispatch("_serialize", c, o, a),
);

/// `$redis->_unserialize(string $value)`
pub const UNSERIALIZE: MethodEntry = MethodEntry::public_async(
    "_unserialize",
    &[Param::new("value", ParamType::String)],
    |c, o, a| dispatch("_unserialize", c, o, a),
);

/// `$redis->_pack(mixed $value)`
pub const PACK: MethodEntry = MethodEntry::public_async(
    "_pack",
    &[Param::new("value", ParamType::Mixed)],
    |c, o, a| dispatch("_pack", c, o, a),
);

/// `$redis->_unpack(string $value)`
pub const UNPACK: MethodEntry = MethodEntry::public_async(
    "_unpack",
    &[Param::new("value", ParamType::String)],
    |c, o, a| dispatch("_unpack", c, o, a),
);

/// `$redis->auth(mixed $credentials)`
pub const AUTH: MethodEntry = MethodEntry::public_async(
    "auth",
    &[Param::new("credentials", ParamType::Mixed)],
    |c, o, a| dispatch("auth", c, o, a),
);

/// `$redis->clearLastError()`
pub const CLEAR_LAST_ERROR: MethodEntry =
    MethodEntry::public_async("clearLastError", &[], |c, o, a| {
        dispatch("clearlasterror", c, o, a)
    });

/// `$redis->close()`
pub const CLOSE: MethodEntry =
    MethodEntry::public_async("close", &[], |c, o, a| dispatch("close", c, o, a));

/// `$redis->connect(string $host, int $port = 6379, float $timeout = 0, ?string $persistent_id = null, int $retry_interval = 0, float $read_timeout = 0, ?array $context = null)`
pub const CONNECT: MethodEntry = MethodEntry::public_async(
    "connect",
    &[
        Param::new("host", ParamType::String),
        Param::new("port", ParamType::Int).optional(),
        Param::new("timeout", ParamType::Float).optional(),
        Param::new("persistent_id", ParamType::String)
            .nullable()
            .optional(),
        Param::new("retry_interval", ParamType::Int).optional(),
        Param::new("read_timeout", ParamType::Float).optional(),
        Param::new("context", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("connect", c, o, a),
);

/// `$redis->getAuth()`
pub const GET_AUTH: MethodEntry =
    MethodEntry::public_async("getAuth", &[], |c, o, a| dispatch("getauth", c, o, a));

/// `$redis->getDBNum()`
pub const GET_DBNUM: MethodEntry =
    MethodEntry::public_async("getDBNum", &[], |c, o, a| dispatch("getdbnum", c, o, a));

/// `$redis->getHost()`
pub const GET_HOST: MethodEntry =
    MethodEntry::public_async("getHost", &[], |c, o, a| dispatch("gethost", c, o, a));

/// `$redis->getLastError()`
pub const GET_LAST_ERROR: MethodEntry =
    MethodEntry::public_async("getLastError", &[], |c, o, a| {
        dispatch("getlasterror", c, o, a)
    });

/// `$redis->getMode()`
pub const GET_MODE: MethodEntry =
    MethodEntry::public_async("getMode", &[], |c, o, a| dispatch("getmode", c, o, a));

/// `$redis->getOption(int $option)`
pub const GET_OPTION: MethodEntry = MethodEntry::public_async(
    "getOption",
    &[Param::new("option", ParamType::Int)],
    |c, o, a| dispatch("getoption", c, o, a),
);

/// `$redis->getPersistentID()`
pub const GET_PERSISTENT_ID: MethodEntry =
    MethodEntry::public_async("getPersistentID", &[], |c, o, a| {
        dispatch("getpersistentid", c, o, a)
    });

/// `$redis->getPort()`
pub const GET_PORT: MethodEntry =
    MethodEntry::public_async("getPort", &[], |c, o, a| dispatch("getport", c, o, a));

/// `$redis->serverName()`
pub const SERVER_NAME: MethodEntry =
    MethodEntry::public_async("serverName", &[], |c, o, a| dispatch("servername", c, o, a));

/// `$redis->serverVersion()`
pub const SERVER_VERSION: MethodEntry =
    MethodEntry::public_async("serverVersion", &[], |c, o, a| {
        dispatch("serverversion", c, o, a)
    });

/// `$redis->getReadTimeout()`
pub const GET_READ_TIMEOUT: MethodEntry =
    MethodEntry::public_async("getReadTimeout", &[], |c, o, a| {
        dispatch("getreadtimeout", c, o, a)
    });

/// `$redis->getTimeout()`
pub const GET_TIMEOUT: MethodEntry =
    MethodEntry::public_async("getTimeout", &[], |c, o, a| dispatch("gettimeout", c, o, a));

/// `$redis->isConnected()`
pub const IS_CONNECTED: MethodEntry = MethodEntry::public_async("isConnected", &[], |c, o, a| {
    dispatch("isconnected", c, o, a)
});

/// `$redis->open(string $host, int $port = 6379, float $timeout = 0, ?string $persistent_id = null, int $retry_interval = 0, float $read_timeout = 0, ?array $context = null)`
pub const OPEN: MethodEntry = MethodEntry::public_async(
    "open",
    &[
        Param::new("host", ParamType::String),
        Param::new("port", ParamType::Int).optional(),
        Param::new("timeout", ParamType::Float).optional(),
        Param::new("persistent_id", ParamType::String)
            .nullable()
            .optional(),
        Param::new("retry_interval", ParamType::Int).optional(),
        Param::new("read_timeout", ParamType::Float).optional(),
        Param::new("context", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("open", c, o, a),
)
.with_preparation(|c, _, _| {
    arguments::deprecated("Redis::open", c);
    Ok(MethodPreparation::Default)
});

/// `$redis->pconnect(string $host, int $port = 6379, float $timeout = 0, ?string $persistent_id = null, int $retry_interval = 0, float $read_timeout = 0, ?array $context = null)`
pub const PCONNECT: MethodEntry = MethodEntry::public_async(
    "pconnect",
    &[
        Param::new("host", ParamType::String),
        Param::new("port", ParamType::Int).optional(),
        Param::new("timeout", ParamType::Float).optional(),
        Param::new("persistent_id", ParamType::String)
            .nullable()
            .optional(),
        Param::new("retry_interval", ParamType::Int).optional(),
        Param::new("read_timeout", ParamType::Float).optional(),
        Param::new("context", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("pconnect", c, o, a),
);

/// `$redis->popen(string $host, int $port = 6379, float $timeout = 0, ?string $persistent_id = null, int $retry_interval = 0, float $read_timeout = 0, ?array $context = null)`
pub const POPEN: MethodEntry = MethodEntry::public_async(
    "popen",
    &[
        Param::new("host", ParamType::String),
        Param::new("port", ParamType::Int).optional(),
        Param::new("timeout", ParamType::Float).optional(),
        Param::new("persistent_id", ParamType::String)
            .nullable()
            .optional(),
        Param::new("retry_interval", ParamType::Int).optional(),
        Param::new("read_timeout", ParamType::Float).optional(),
        Param::new("context", ParamType::Array)
            .nullable()
            .optional(),
    ],
    |c, o, a| dispatch("popen", c, o, a),
)
.with_preparation(|c, _, _| {
    arguments::deprecated("Redis::popen", c);
    Ok(MethodPreparation::Default)
});

/// `$redis->select(int $db)`
pub const SELECT: MethodEntry =
    MethodEntry::public_async("select", &[Param::new("db", ParamType::Int)], |c, o, a| {
        dispatch("select", c, o, a)
    });

/// `$redis->setOption(int $option, mixed $value)`
pub const SET_OPTION: MethodEntry = MethodEntry::public_async(
    "setOption",
    &[
        Param::new("option", ParamType::Int),
        Param::new("value", ParamType::Mixed),
    ],
    |c, o, a| dispatch("setoption", c, o, a),
);
