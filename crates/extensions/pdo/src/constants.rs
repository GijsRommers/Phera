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
pub const ATTR_ERRMODE: i64 = 3;
pub const ATTR_SERVER_VERSION: i64 = 4;
pub const ATTR_CASE: i64 = 8;
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

pub const CONSTANTS: &[(&str, ConstantValue)] = &[
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
    ("ATTR_TIMEOUT", ConstantValue::Int(2)),
    ("ATTR_ERRMODE", ConstantValue::Int(ATTR_ERRMODE)),
    (
        "ATTR_SERVER_VERSION",
        ConstantValue::Int(ATTR_SERVER_VERSION),
    ),
    ("ATTR_CLIENT_VERSION", ConstantValue::Int(5)),
    ("ATTR_SERVER_INFO", ConstantValue::Int(6)),
    ("ATTR_CONNECTION_STATUS", ConstantValue::Int(7)),
    ("ATTR_CASE", ConstantValue::Int(ATTR_CASE)),
    ("ATTR_CURSOR_NAME", ConstantValue::Int(9)),
    ("ATTR_CURSOR", ConstantValue::Int(10)),
    ("ATTR_ORACLE_NULLS", ConstantValue::Int(11)),
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
