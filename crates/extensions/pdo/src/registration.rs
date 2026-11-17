//! Native method signatures match PHP: `$pdo->query(query: 'SELECT 1', fetchMode: PDO::FETCH_NUM)`.

use phera_extension_api::{
    ClassEntry, ConstantValue, FunctionEntry, MethodEntry, Param, ParamType, PropertyEntry,
    Registry, Visibility,
};

use crate::{attributes, connection, constants, fetch, statement, transactions};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        parent: Some("RuntimeException"),
        properties: const {
            &[PropertyEntry::new(
                "errorInfo",
                ConstantValue::Null,
                Visibility::Public,
            )]
        },
        ..ClassEntry::class("PDOException")
    });
    registry.class(ClassEntry {
        constants: constants::CONSTANTS,
        methods: PDO_METHODS,
        ..ClassEntry::class("PDO")
    });
    for name in ["Pdo\\Pgsql", "Pdo\\Mysql"] {
        registry.class(ClassEntry {
            parent: Some("PDO"),
            methods: PDO_METHODS,
            ..ClassEntry::class(name)
        });
    }
    registry.class(ClassEntry {
        properties: const {
            &[PropertyEntry::new(
                "queryString",
                ConstantValue::String(""),
                Visibility::Public,
            )]
        },
        methods: STATEMENT_METHODS,
        ..ClassEntry::class("PDOStatement")
    });
    registry.function(FunctionEntry::sync("pdo_drivers", &[], connection::drivers));
}

const PDO_METHODS: &[MethodEntry] = &[
    MethodEntry::public_async(
        "__construct",
        &[
            Param::new("dsn", ParamType::String),
            Param::new("username", ParamType::String)
                .nullable()
                .optional(),
            Param::new("password", ParamType::String)
                .nullable()
                .optional(),
            Param::new("options", ParamType::Array)
                .nullable()
                .optional(),
        ],
        connection::construct,
    ),
    MethodEntry::public_async(
        "connect",
        &[
            Param::new("dsn", ParamType::String),
            Param::new("username", ParamType::String)
                .nullable()
                .optional(),
            Param::new("password", ParamType::String)
                .nullable()
                .optional(),
            Param::new("options", ParamType::Array)
                .nullable()
                .optional(),
        ],
        connection::connect,
    )
    .static_(),
    MethodEntry::public("getAvailableDrivers", &[], connection::available).static_(),
    MethodEntry::public(
        "pgsqlCopyFromArray",
        &[Param::new("args", ParamType::Mixed).variadic()],
        statement::unsupported_method,
    ),
    MethodEntry::public(
        "pgsqlCopyFromFile",
        &[Param::new("args", ParamType::Mixed).variadic()],
        statement::unsupported_method,
    ),
    MethodEntry::public(
        "pgsqlCopyToArray",
        &[Param::new("args", ParamType::Mixed).variadic()],
        statement::unsupported_method,
    ),
    MethodEntry::public(
        "pgsqlCopyToFile",
        &[Param::new("args", ParamType::Mixed).variadic()],
        statement::unsupported_method,
    ),
    MethodEntry::public_async("beginTransaction", &[], transactions::begin),
    MethodEntry::public_async("commit", &[], transactions::commit),
    MethodEntry::public_async("rollBack", &[], transactions::rollback),
    MethodEntry::public_async("inTransaction", &[], transactions::active),
    MethodEntry::public("errorCode", &[], connection::error_code),
    MethodEntry::public("errorInfo", &[], connection::error_info),
    MethodEntry::public_async(
        "exec",
        &[Param::new("statement", ParamType::String)],
        connection::exec,
    ),
    MethodEntry::public_async(
        "prepare",
        &[
            Param::new("query", ParamType::String),
            Param::new("options", ParamType::Array).optional(),
        ],
        statement::prepare,
    ),
    MethodEntry::public_async(
        "query",
        &[
            Param::new("query", ParamType::String),
            Param::new("fetchMode", ParamType::Int)
                .nullable()
                .optional(),
            Param::new("fetchModeArgs", ParamType::Mixed).variadic(),
        ],
        statement::query,
    ),
    MethodEntry::public_async(
        "quote",
        &[
            Param::new("string", ParamType::String),
            Param::new("type", ParamType::Int).optional(),
        ],
        connection::quote,
    ),
    MethodEntry::public_async(
        "lastInsertId",
        &[Param::new("name", ParamType::String).nullable().optional()],
        connection::last_id,
    ),
    MethodEntry::public(
        "setAttribute",
        &[
            Param::new("attribute", ParamType::Int),
            Param::new("value", ParamType::Mixed),
        ],
        attributes::set,
    ),
    MethodEntry::public(
        "getAttribute",
        &[Param::new("attribute", ParamType::Int)],
        attributes::get,
    ),
];

const STATEMENT_METHODS: &[MethodEntry] = &[
    MethodEntry::public_async(
        "execute",
        &[Param::new("params", ParamType::Array).nullable().optional()],
        crate::statement_execute::execute,
    ),
    MethodEntry::public(
        "bindValue",
        &[
            Param::new("param", ParamType::IntOrString),
            Param::new("value", ParamType::Mixed),
            Param::new("type", ParamType::Int).optional(),
        ],
        statement::bind_value,
    ),
    MethodEntry::public(
        "bindParam",
        &[
            Param::new("param", ParamType::IntOrString),
            Param::new("var", ParamType::Mixed).by_ref(),
            Param::new("type", ParamType::Int).optional(),
            Param::new("maxLength", ParamType::Int).optional(),
            Param::new("driverOptions", ParamType::Mixed)
                .nullable()
                .optional(),
        ],
        statement::bind_param,
    ),
    MethodEntry::public(
        "fetch",
        &[
            Param::new("mode", ParamType::Int).optional(),
            Param::new("cursorOrientation", ParamType::Int).optional(),
            Param::new("cursorOffset", ParamType::Int).optional(),
        ],
        fetch::fetch,
    ),
    MethodEntry::public(
        "fetchAll",
        &[
            Param::new("mode", ParamType::Int).optional(),
            Param::new("args", ParamType::Mixed).variadic(),
        ],
        crate::fetch_all::all,
    ),
    MethodEntry::public(
        "fetchColumn",
        &[Param::new("column", ParamType::Int).optional()],
        fetch::column,
    ),
    MethodEntry::public(
        "setFetchMode",
        &[
            Param::new("mode", ParamType::Int),
            Param::new("args", ParamType::Mixed).variadic(),
        ],
        fetch::set_mode,
    ),
    MethodEntry::public("closeCursor", &[], statement::close),
    MethodEntry::public("columnCount", &[], statement::column_count),
    MethodEntry::public("rowCount", &[], statement::row_count),
    MethodEntry::public("errorCode", &[], statement::error_code),
    MethodEntry::public("errorInfo", &[], statement::error_info),
    MethodEntry::public(
        "fetchObject",
        &[
            Param::new("class", ParamType::String).nullable().optional(),
            Param::new("constructorArgs", ParamType::Array).optional(),
        ],
        fetch::object,
    ),
    MethodEntry::public(
        "bindColumn",
        &[Param::new("args", ParamType::Mixed).variadic()],
        statement::unsupported_method,
    ),
    MethodEntry::public_async(
        "getColumnMeta",
        &[Param::new("column", ParamType::Int)],
        crate::metadata::get,
    ),
    MethodEntry::public(
        "nextRowset",
        &[Param::new("args", ParamType::Mixed).variadic()],
        statement::unsupported_method,
    ),
    MethodEntry::public(
        "getIterator",
        &[Param::new("args", ParamType::Mixed).variadic()],
        statement::unsupported_method,
    ),
    MethodEntry::public(
        "debugDumpParams",
        &[Param::new("args", ParamType::Mixed).variadic()],
        statement::unsupported_method,
    ),
    MethodEntry::public(
        "getAttribute",
        &[Param::new("args", ParamType::Mixed).variadic()],
        statement::unsupported_method,
    ),
    MethodEntry::public(
        "setAttribute",
        &[Param::new("args", ParamType::Mixed).variadic()],
        statement::unsupported_method,
    ),
];
