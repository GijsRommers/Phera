//! PDO's class constants: `$statement->fetch(PDO::FETCH_ASSOC)` returns a
//! row keyed by column name. The ones the code itself compares with have a
//! Rust name too, so the numbers are written once.

use phera_extension_api::ConstantValue;

pub const PARAM_NULL: i64 = 0;
pub const PARAM_INT: i64 = 1;
pub const PARAM_STR: i64 = 2;
pub const PARAM_LOB: i64 = 3;
pub const PARAM_BOOL: i64 = 5;
pub const FETCH_DEFAULT: i64 = 0;
pub const FETCH_ASSOC: i64 = 2;
pub const FETCH_NUM: i64 = 3;
pub const FETCH_BOTH: i64 = 4;
pub const FETCH_OBJ: i64 = 5;
pub const FETCH_COLUMN: i64 = 7;
pub const FETCH_CLASS: i64 = 8;
pub const FETCH_KEY_PAIR: i64 = 12;
pub const FETCH_GROUP: i64 = 32;
pub const FETCH_UNIQUE: i64 = 64;
pub const FETCH_CLASSTYPE: i64 = 128;
pub const FETCH_PROPS_LATE: i64 = 256;
pub const FETCH_SERIALIZE: i64 = 512;
pub const ATTR_TIMEOUT: i64 = 2;
pub const ATTR_ERRMODE: i64 = 3;
pub const ATTR_SERVER_VERSION: i64 = 4;
pub const ATTR_CLIENT_VERSION: i64 = 5;
pub const ATTR_CASE: i64 = 8;
pub const ATTR_ORACLE_NULLS: i64 = 11;
pub const ATTR_PERSISTENT: i64 = 12;
pub const ATTR_DRIVER_NAME: i64 = 16;
pub const ATTR_STRINGIFY_FETCHES: i64 = 17;
pub const ATTR_DEFAULT_FETCH_MODE: i64 = 19;
pub const ATTR_EMULATE_PREPARES: i64 = 20;
pub const ERRMODE_SILENT: i64 = 0;
pub const ERRMODE_WARNING: i64 = 1;
pub const ERRMODE_EXCEPTION: i64 = 2;
pub const CASE_NATURAL: i64 = 0;
pub const CASE_UPPER: i64 = 1;
pub const CASE_LOWER: i64 = 2;

/// Every bit that a fetch mode argument may use.
pub const FETCH_KNOWN_BITS: i64 = 0x3ff;
/// The low bits hold the mode (`FETCH_ASSOC` ...), the high bits its flags.
pub const FETCH_MODE_BITS: i64 = 0x1f;
/// The flags that only `FETCH_CLASS` takes.
pub const FETCH_CLASS_FLAGS: i64 = FETCH_CLASSTYPE | FETCH_PROPS_LATE | FETCH_SERIALIZE;
/// The flags `fetchAll()` groups rows by.
pub const FETCH_GROUPING: i64 = FETCH_GROUP | FETCH_UNIQUE;

/// `Pdo\Sqlite::ATTR_*`: the attributes only SQLite has, and its open flags.
pub const SQLITE_ATTR_OPEN_FLAGS: i64 = 1000;
pub const SQLITE_ATTR_READONLY_STATEMENT: i64 = 1001;
pub const SQLITE_ATTR_EXTENDED_RESULT_CODES: i64 = 1002;
pub const SQLITE_ATTR_BUSY_STATEMENT: i64 = 1003;
pub const SQLITE_ATTR_EXPLAIN_STATEMENT: i64 = 1004;
pub const SQLITE_ATTR_TRANSACTION_MODE: i64 = 1005;
pub const SQLITE_OPEN_READONLY: i64 = 1;
pub const SQLITE_OPEN_READWRITE: i64 = 2;
pub const SQLITE_OPEN_CREATE: i64 = 4;
pub const SQLITE_TRANSACTION_MODE_IMMEDIATE: i64 = 1;
pub const SQLITE_TRANSACTION_MODE_EXCLUSIVE: i64 = 2;

pub const CONSTANTS: &[(&str, ConstantValue)] = &[
    // `PDO::MYSQL_*` and `PDO::PGSQL_*`: deprecated in PHP 8.5 in favour of the
    // constants of `Pdo\Mysql` and `Pdo\Pgsql`, and still what most database
    // configuration files write; see DRIVER_DEPRECATED.
    ("MYSQL_ATTR_USE_BUFFERED_QUERY", ConstantValue::Int(1000)),
    ("MYSQL_ATTR_LOCAL_INFILE", ConstantValue::Int(1001)),
    ("MYSQL_ATTR_INIT_COMMAND", ConstantValue::Int(1002)),
    ("MYSQL_ATTR_COMPRESS", ConstantValue::Int(1003)),
    ("MYSQL_ATTR_DIRECT_QUERY", ConstantValue::Int(20)),
    ("MYSQL_ATTR_FOUND_ROWS", ConstantValue::Int(1004)),
    ("MYSQL_ATTR_IGNORE_SPACE", ConstantValue::Int(1005)),
    ("MYSQL_ATTR_SSL_KEY", ConstantValue::Int(1006)),
    ("MYSQL_ATTR_SSL_CERT", ConstantValue::Int(1007)),
    ("MYSQL_ATTR_SSL_CA", ConstantValue::Int(1008)),
    ("MYSQL_ATTR_SSL_CAPATH", ConstantValue::Int(1009)),
    ("MYSQL_ATTR_SSL_CIPHER", ConstantValue::Int(1010)),
    ("MYSQL_ATTR_SERVER_PUBLIC_KEY", ConstantValue::Int(1011)),
    ("MYSQL_ATTR_MULTI_STATEMENTS", ConstantValue::Int(1012)),
    (
        "MYSQL_ATTR_SSL_VERIFY_SERVER_CERT",
        ConstantValue::Int(1013),
    ),
    (
        "MYSQL_ATTR_LOCAL_INFILE_DIRECTORY",
        ConstantValue::Int(1014),
    ),
    ("PGSQL_ATTR_DISABLE_PREPARES", ConstantValue::Int(1000)),
    ("PGSQL_TRANSACTION_IDLE", ConstantValue::Int(0)),
    ("PGSQL_TRANSACTION_ACTIVE", ConstantValue::Int(1)),
    ("PGSQL_TRANSACTION_INTRANS", ConstantValue::Int(2)),
    ("PGSQL_TRANSACTION_INERROR", ConstantValue::Int(3)),
    ("PGSQL_TRANSACTION_UNKNOWN", ConstantValue::Int(4)),
    // Deprecated in PHP 8.5 in favour of `Pdo\Sqlite`'s; see DRIVER_DEPRECATED.
    ("SQLITE_DETERMINISTIC", ConstantValue::Int(2048)),
    (
        "SQLITE_ATTR_OPEN_FLAGS",
        ConstantValue::Int(SQLITE_ATTR_OPEN_FLAGS),
    ),
    (
        "SQLITE_ATTR_READONLY_STATEMENT",
        ConstantValue::Int(SQLITE_ATTR_READONLY_STATEMENT),
    ),
    (
        "SQLITE_ATTR_EXTENDED_RESULT_CODES",
        ConstantValue::Int(SQLITE_ATTR_EXTENDED_RESULT_CODES),
    ),
    (
        "SQLITE_OPEN_READONLY",
        ConstantValue::Int(SQLITE_OPEN_READONLY),
    ),
    (
        "SQLITE_OPEN_READWRITE",
        ConstantValue::Int(SQLITE_OPEN_READWRITE),
    ),
    ("SQLITE_OPEN_CREATE", ConstantValue::Int(SQLITE_OPEN_CREATE)),
    (
        "PHERA_ATTR_POOL",
        ConstantValue::Int(crate::pool::PHERA_ATTR_POOL),
    ),
    ("PARAM_NULL", ConstantValue::Int(PARAM_NULL)),
    ("PARAM_BOOL", ConstantValue::Int(PARAM_BOOL)),
    ("PARAM_INT", ConstantValue::Int(PARAM_INT)),
    ("PARAM_STR", ConstantValue::Int(PARAM_STR)),
    ("PARAM_LOB", ConstantValue::Int(PARAM_LOB)),
    ("PARAM_STMT", ConstantValue::Int(4)),
    ("PARAM_INPUT_OUTPUT", ConstantValue::Int(2147483648)),
    ("PARAM_STR_NATL", ConstantValue::Int(1073741824)),
    ("PARAM_STR_CHAR", ConstantValue::Int(536870912)),
    ("PARAM_EVT_ALLOC", ConstantValue::Int(0)),
    ("PARAM_EVT_FREE", ConstantValue::Int(1)),
    ("PARAM_EVT_EXEC_PRE", ConstantValue::Int(2)),
    ("PARAM_EVT_EXEC_POST", ConstantValue::Int(3)),
    ("PARAM_EVT_FETCH_PRE", ConstantValue::Int(4)),
    ("PARAM_EVT_FETCH_POST", ConstantValue::Int(5)),
    ("PARAM_EVT_NORMALIZE", ConstantValue::Int(6)),
    ("FETCH_DEFAULT", ConstantValue::Int(FETCH_DEFAULT)),
    ("FETCH_LAZY", ConstantValue::Int(1)),
    ("FETCH_ASSOC", ConstantValue::Int(FETCH_ASSOC)),
    ("FETCH_NUM", ConstantValue::Int(FETCH_NUM)),
    ("FETCH_BOTH", ConstantValue::Int(FETCH_BOTH)),
    ("FETCH_OBJ", ConstantValue::Int(FETCH_OBJ)),
    ("FETCH_BOUND", ConstantValue::Int(6)),
    ("FETCH_COLUMN", ConstantValue::Int(FETCH_COLUMN)),
    ("FETCH_CLASS", ConstantValue::Int(FETCH_CLASS)),
    ("FETCH_INTO", ConstantValue::Int(9)),
    ("FETCH_FUNC", ConstantValue::Int(10)),
    ("FETCH_GROUP", ConstantValue::Int(FETCH_GROUP)),
    ("FETCH_UNIQUE", ConstantValue::Int(FETCH_UNIQUE)),
    ("FETCH_KEY_PAIR", ConstantValue::Int(FETCH_KEY_PAIR)),
    ("FETCH_CLASSTYPE", ConstantValue::Int(FETCH_CLASSTYPE)),
    ("FETCH_SERIALIZE", ConstantValue::Int(FETCH_SERIALIZE)),
    ("FETCH_PROPS_LATE", ConstantValue::Int(FETCH_PROPS_LATE)),
    ("FETCH_NAMED", ConstantValue::Int(11)),
    ("ATTR_AUTOCOMMIT", ConstantValue::Int(0)),
    ("ATTR_PREFETCH", ConstantValue::Int(1)),
    ("ATTR_TIMEOUT", ConstantValue::Int(ATTR_TIMEOUT)),
    ("ATTR_ERRMODE", ConstantValue::Int(ATTR_ERRMODE)),
    (
        "ATTR_SERVER_VERSION",
        ConstantValue::Int(ATTR_SERVER_VERSION),
    ),
    (
        "ATTR_CLIENT_VERSION",
        ConstantValue::Int(ATTR_CLIENT_VERSION),
    ),
    ("ATTR_SERVER_INFO", ConstantValue::Int(6)),
    ("ATTR_CONNECTION_STATUS", ConstantValue::Int(7)),
    ("ATTR_CASE", ConstantValue::Int(ATTR_CASE)),
    ("ATTR_CURSOR_NAME", ConstantValue::Int(9)),
    ("ATTR_CURSOR", ConstantValue::Int(10)),
    ("ATTR_ORACLE_NULLS", ConstantValue::Int(ATTR_ORACLE_NULLS)),
    ("ATTR_PERSISTENT", ConstantValue::Int(ATTR_PERSISTENT)),
    ("ATTR_STATEMENT_CLASS", ConstantValue::Int(13)),
    ("ATTR_FETCH_TABLE_NAMES", ConstantValue::Int(14)),
    ("ATTR_FETCH_CATALOG_NAMES", ConstantValue::Int(15)),
    ("ATTR_DRIVER_NAME", ConstantValue::Int(ATTR_DRIVER_NAME)),
    (
        "ATTR_STRINGIFY_FETCHES",
        ConstantValue::Int(ATTR_STRINGIFY_FETCHES),
    ),
    ("ATTR_MAX_COLUMN_LEN", ConstantValue::Int(18)),
    (
        "ATTR_EMULATE_PREPARES",
        ConstantValue::Int(ATTR_EMULATE_PREPARES),
    ),
    (
        "ATTR_DEFAULT_FETCH_MODE",
        ConstantValue::Int(ATTR_DEFAULT_FETCH_MODE),
    ),
    ("ATTR_DEFAULT_STR_PARAM", ConstantValue::Int(21)),
    ("ERRMODE_SILENT", ConstantValue::Int(ERRMODE_SILENT)),
    ("ERRMODE_WARNING", ConstantValue::Int(ERRMODE_WARNING)),
    ("ERRMODE_EXCEPTION", ConstantValue::Int(ERRMODE_EXCEPTION)),
    ("CASE_NATURAL", ConstantValue::Int(CASE_NATURAL)),
    ("CASE_LOWER", ConstantValue::Int(CASE_LOWER)),
    ("CASE_UPPER", ConstantValue::Int(CASE_UPPER)),
    ("NULL_NATURAL", ConstantValue::Int(0)),
    ("NULL_EMPTY_STRING", ConstantValue::Int(1)),
    ("NULL_TO_STRING", ConstantValue::Int(2)),
    ("ERR_NONE", ConstantValue::String("00000")),
    ("FETCH_ORI_NEXT", ConstantValue::Int(0)),
    ("FETCH_ORI_PRIOR", ConstantValue::Int(1)),
    ("FETCH_ORI_FIRST", ConstantValue::Int(2)),
    ("FETCH_ORI_LAST", ConstantValue::Int(3)),
    ("FETCH_ORI_ABS", ConstantValue::Int(4)),
    ("FETCH_ORI_REL", ConstantValue::Int(5)),
    ("CURSOR_FWDONLY", ConstantValue::Int(0)),
    ("CURSOR_SCROLL", ConstantValue::Int(1)),
];

/// `Pdo\Sqlite`'s class constants.
/// The constants of `Pdo\\Mysql`, with PHP 8.5's values.
pub const MYSQL: &[(&str, ConstantValue)] = &[
    ("ATTR_USE_BUFFERED_QUERY", ConstantValue::Int(1000)),
    ("ATTR_LOCAL_INFILE", ConstantValue::Int(1001)),
    ("ATTR_INIT_COMMAND", ConstantValue::Int(1002)),
    ("ATTR_COMPRESS", ConstantValue::Int(1003)),
    ("ATTR_DIRECT_QUERY", ConstantValue::Int(20)),
    ("ATTR_FOUND_ROWS", ConstantValue::Int(1004)),
    ("ATTR_IGNORE_SPACE", ConstantValue::Int(1005)),
    ("ATTR_SSL_KEY", ConstantValue::Int(1006)),
    ("ATTR_SSL_CERT", ConstantValue::Int(1007)),
    ("ATTR_SSL_CA", ConstantValue::Int(1008)),
    ("ATTR_SSL_CAPATH", ConstantValue::Int(1009)),
    ("ATTR_SSL_CIPHER", ConstantValue::Int(1010)),
    ("ATTR_SERVER_PUBLIC_KEY", ConstantValue::Int(1011)),
    ("ATTR_MULTI_STATEMENTS", ConstantValue::Int(1012)),
    ("ATTR_SSL_VERIFY_SERVER_CERT", ConstantValue::Int(1013)),
    ("ATTR_LOCAL_INFILE_DIRECTORY", ConstantValue::Int(1014)),
];

/// The constants of `Pdo\\Pgsql`, with PHP 8.5's values.
pub const PGSQL: &[(&str, ConstantValue)] = &[
    ("ATTR_DISABLE_PREPARES", ConstantValue::Int(1000)),
    ("ATTR_RESULT_MEMORY_SIZE", ConstantValue::Int(1001)),
];

pub const SQLITE: &[(&str, ConstantValue)] = &[
    ("DETERMINISTIC", ConstantValue::Int(2048)),
    ("OPEN_READONLY", ConstantValue::Int(SQLITE_OPEN_READONLY)),
    ("OPEN_READWRITE", ConstantValue::Int(SQLITE_OPEN_READWRITE)),
    ("OPEN_CREATE", ConstantValue::Int(SQLITE_OPEN_CREATE)),
    (
        "ATTR_OPEN_FLAGS",
        ConstantValue::Int(SQLITE_ATTR_OPEN_FLAGS),
    ),
    (
        "ATTR_READONLY_STATEMENT",
        ConstantValue::Int(SQLITE_ATTR_READONLY_STATEMENT),
    ),
    (
        "ATTR_EXTENDED_RESULT_CODES",
        ConstantValue::Int(SQLITE_ATTR_EXTENDED_RESULT_CODES),
    ),
    (
        "ATTR_BUSY_STATEMENT",
        ConstantValue::Int(SQLITE_ATTR_BUSY_STATEMENT),
    ),
    (
        "ATTR_EXPLAIN_STATEMENT",
        ConstantValue::Int(SQLITE_ATTR_EXPLAIN_STATEMENT),
    ),
    (
        "ATTR_TRANSACTION_MODE",
        ConstantValue::Int(SQLITE_ATTR_TRANSACTION_MODE),
    ),
    ("TRANSACTION_MODE_DEFERRED", ConstantValue::Int(0)),
    (
        "TRANSACTION_MODE_IMMEDIATE",
        ConstantValue::Int(SQLITE_TRANSACTION_MODE_IMMEDIATE),
    ),
    (
        "TRANSACTION_MODE_EXCLUSIVE",
        ConstantValue::Int(SQLITE_TRANSACTION_MODE_EXCLUSIVE),
    ),
    ("EXPLAIN_MODE_PREPARED", ConstantValue::Int(0)),
    ("EXPLAIN_MODE_EXPLAIN", ConstantValue::Int(1)),
    ("EXPLAIN_MODE_EXPLAIN_QUERY_PLAN", ConstantValue::Int(2)),
    ("OK", ConstantValue::Int(0)),
    ("DENY", ConstantValue::Int(1)),
    ("IGNORE", ConstantValue::Int(2)),
];

/// The driver constants on `PDO` itself (`PDO::SQLITE_*`, `PDO::MYSQL_*`,
/// `PDO::PGSQL_*`) warn on use, as in PHP 8.5. The messages are PHP's.
pub const DRIVER_DEPRECATED: &[(&str, &str)] = &[
    (
        "MYSQL_ATTR_USE_BUFFERED_QUERY",
        "Constant PDO::MYSQL_ATTR_USE_BUFFERED_QUERY is deprecated since 8.5, use Pdo\\Mysql::ATTR_USE_BUFFERED_QUERY instead",
    ),
    (
        "MYSQL_ATTR_LOCAL_INFILE",
        "Constant PDO::MYSQL_ATTR_LOCAL_INFILE is deprecated since 8.5, use Pdo\\Mysql::ATTR_LOCAL_INFILE instead",
    ),
    (
        "MYSQL_ATTR_INIT_COMMAND",
        "Constant PDO::MYSQL_ATTR_INIT_COMMAND is deprecated since 8.5, use Pdo\\Mysql::ATTR_INIT_COMMAND instead",
    ),
    (
        "MYSQL_ATTR_COMPRESS",
        "Constant PDO::MYSQL_ATTR_COMPRESS is deprecated since 8.5, use Pdo\\Mysql::ATTR_COMPRESS instead",
    ),
    (
        "MYSQL_ATTR_DIRECT_QUERY",
        "Constant PDO::MYSQL_ATTR_DIRECT_QUERY is deprecated since 8.5, use Pdo\\Mysql::ATTR_DIRECT_QUERY instead",
    ),
    (
        "MYSQL_ATTR_FOUND_ROWS",
        "Constant PDO::MYSQL_ATTR_FOUND_ROWS is deprecated since 8.5, use Pdo\\Mysql::ATTR_FOUND_ROWS instead",
    ),
    (
        "MYSQL_ATTR_IGNORE_SPACE",
        "Constant PDO::MYSQL_ATTR_IGNORE_SPACE is deprecated since 8.5, use Pdo\\Mysql::ATTR_IGNORE_SPACE instead",
    ),
    (
        "MYSQL_ATTR_SSL_KEY",
        "Constant PDO::MYSQL_ATTR_SSL_KEY is deprecated since 8.5, use Pdo\\Mysql::ATTR_SSL_KEY instead",
    ),
    (
        "MYSQL_ATTR_SSL_CERT",
        "Constant PDO::MYSQL_ATTR_SSL_CERT is deprecated since 8.5, use Pdo\\Mysql::ATTR_SSL_CERT instead",
    ),
    (
        "MYSQL_ATTR_SSL_CA",
        "Constant PDO::MYSQL_ATTR_SSL_CA is deprecated since 8.5, use Pdo\\Mysql::ATTR_SSL_CA instead",
    ),
    (
        "MYSQL_ATTR_SSL_CAPATH",
        "Constant PDO::MYSQL_ATTR_SSL_CAPATH is deprecated since 8.5, use Pdo\\Mysql::ATTR_SSL_CAPATH instead",
    ),
    (
        "MYSQL_ATTR_SSL_CIPHER",
        "Constant PDO::MYSQL_ATTR_SSL_CIPHER is deprecated since 8.5, use Pdo\\Mysql::ATTR_SSL_CIPHER instead",
    ),
    (
        "MYSQL_ATTR_SERVER_PUBLIC_KEY",
        "Constant PDO::MYSQL_ATTR_SERVER_PUBLIC_KEY is deprecated since 8.5, use Pdo\\Mysql::ATTR_SERVER_PUBLIC_KEY instead",
    ),
    (
        "MYSQL_ATTR_MULTI_STATEMENTS",
        "Constant PDO::MYSQL_ATTR_MULTI_STATEMENTS is deprecated since 8.5, use Pdo\\Mysql::ATTR_MULTI_STATEMENTS instead",
    ),
    (
        "MYSQL_ATTR_SSL_VERIFY_SERVER_CERT",
        "Constant PDO::MYSQL_ATTR_SSL_VERIFY_SERVER_CERT is deprecated since 8.5, use Pdo\\Mysql::ATTR_SSL_VERIFY_SERVER_CERT instead",
    ),
    (
        "MYSQL_ATTR_LOCAL_INFILE_DIRECTORY",
        "Constant PDO::MYSQL_ATTR_LOCAL_INFILE_DIRECTORY is deprecated since 8.5, use Pdo\\Mysql::ATTR_LOCAL_INFILE_DIRECTORY instead",
    ),
    (
        "PGSQL_ATTR_DISABLE_PREPARES",
        "Constant PDO::PGSQL_ATTR_DISABLE_PREPARES is deprecated since 8.5, use Pdo\\Pgsql::ATTR_DISABLE_PREPARES instead",
    ),
    (
        "PGSQL_TRANSACTION_IDLE",
        "Constant PDO::PGSQL_TRANSACTION_IDLE is deprecated since 8.5",
    ),
    (
        "PGSQL_TRANSACTION_ACTIVE",
        "Constant PDO::PGSQL_TRANSACTION_ACTIVE is deprecated since 8.5",
    ),
    (
        "PGSQL_TRANSACTION_INTRANS",
        "Constant PDO::PGSQL_TRANSACTION_INTRANS is deprecated since 8.5",
    ),
    (
        "PGSQL_TRANSACTION_INERROR",
        "Constant PDO::PGSQL_TRANSACTION_INERROR is deprecated since 8.5",
    ),
    (
        "PGSQL_TRANSACTION_UNKNOWN",
        "Constant PDO::PGSQL_TRANSACTION_UNKNOWN is deprecated since 8.5",
    ),
    (
        "SQLITE_DETERMINISTIC",
        "Constant PDO::SQLITE_DETERMINISTIC is deprecated since 8.5, use Pdo\\Sqlite::DETERMINISTIC instead",
    ),
    (
        "SQLITE_ATTR_OPEN_FLAGS",
        "Constant PDO::SQLITE_ATTR_OPEN_FLAGS is deprecated since 8.5, use Pdo\\Sqlite::ATTR_OPEN_FLAGS instead",
    ),
    (
        "SQLITE_ATTR_READONLY_STATEMENT",
        "Constant PDO::SQLITE_ATTR_READONLY_STATEMENT is deprecated since 8.5, use Pdo\\Sqlite::ATTR_READONLY_STATEMENT instead",
    ),
    (
        "SQLITE_ATTR_EXTENDED_RESULT_CODES",
        "Constant PDO::SQLITE_ATTR_EXTENDED_RESULT_CODES is deprecated since 8.5, use Pdo\\Sqlite::ATTR_EXTENDED_RESULT_CODES instead",
    ),
    (
        "SQLITE_OPEN_READONLY",
        "Constant PDO::SQLITE_OPEN_READONLY is deprecated since 8.5, use Pdo\\Sqlite::OPEN_READONLY instead",
    ),
    (
        "SQLITE_OPEN_READWRITE",
        "Constant PDO::SQLITE_OPEN_READWRITE is deprecated since 8.5, use Pdo\\Sqlite::OPEN_READWRITE instead",
    ),
    (
        "SQLITE_OPEN_CREATE",
        "Constant PDO::SQLITE_OPEN_CREATE is deprecated since 8.5, use Pdo\\Sqlite::OPEN_CREATE instead",
    ),
];
