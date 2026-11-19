//! Native method signatures match PHP: `$pdo->query(query: 'SELECT 1', fetchMode: PDO::FETCH_NUM)`.

use phera_extension_api::{
    ClassEntry, ClassHooks, ConstantValue, FunctionEntry, MethodEntry, NativeClone, Param,
    ParamType, PropertyEntry, Registry, Visibility,
};

use crate::{attributes, connection, constants, fetch, statement, transactions};

static UNCOPYABLE_CONNECTION: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Forbidden),
    ..ClassHooks::EMPTY
};

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
        deprecated_constants: constants::DRIVER_DEPRECATED,
        methods: PDO_METHODS,
        hooks: Some(&UNCOPYABLE_CONNECTION),
        ..ClassEntry::class("PDO")
    });
    for (name, constants) in [
        ("Pdo\\Pgsql", constants::PGSQL),
        ("Pdo\\Mysql", constants::MYSQL),
    ] {
        registry.class(ClassEntry {
            parent: Some("PDO"),
            constants,
            ..ClassEntry::class(name)
        });
    }
    registry.class(ClassEntry {
        parent: Some("PDO"),
        constants: crate::constants::SQLITE,
        methods: SQLITE_METHODS,
        ..ClassEntry::class("Pdo\\Sqlite")
    });
    registry.class(ClassEntry {
        properties: const {
            &[PropertyEntry::new(
                "queryString",
                ConstantValue::String(""),
                Visibility::Public,
            )]
        },
        methods: STATEMENT_METHODS,
        hooks: Some(&UNCOPYABLE_CONNECTION),
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
                .with_default(ConstantValue::Null),
            Param::new("password", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
            Param::new("options", ParamType::Array)
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        connection::construct,
    ),
    MethodEntry::public_async(
        "connect",
        &[
            Param::new("dsn", ParamType::String),
            Param::new("username", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
            Param::new("password", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
            Param::new("options", ParamType::Array)
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        connection::connect,
    )
    .static_()
    .returning("static"),
    MethodEntry::public_async("beginTransaction", &[], transactions::begin)
        .tentative_return("bool"),
    MethodEntry::public_async("commit", &[], transactions::commit).tentative_return("bool"),
    MethodEntry::public("errorCode", &[], connection::error_code).tentative_return("?string"),
    MethodEntry::public("errorInfo", &[], connection::error_info).tentative_return("array"),
    MethodEntry::public_async(
        "exec",
        &[Param::new("statement", ParamType::String)],
        connection::exec,
    )
    .tentative_return("int|false"),
    MethodEntry::public_async(
        "getAttribute",
        &[Param::new("attribute", ParamType::Int)],
        attributes::get,
    )
    .tentative_return("mixed"),
    MethodEntry::public("getAvailableDrivers", &[], connection::available)
        .static_()
        .tentative_return("array"),
    MethodEntry::public_async("inTransaction", &[], transactions::active).tentative_return("bool"),
    MethodEntry::public_async(
        "lastInsertId",
        &[Param::new("name", ParamType::String)
            .nullable()
            .with_default(ConstantValue::Null)],
        connection::last_id,
    )
    .tentative_return("string|false"),
    MethodEntry::public_async(
        "prepare",
        &[
            Param::new("query", ParamType::String),
            Param::new("options", ParamType::Array).with_default(ConstantValue::EmptyArray),
        ],
        statement::prepare,
    )
    .tentative_return("PDOStatement|false"),
    MethodEntry::public_async(
        "query",
        &[
            Param::new("query", ParamType::String),
            Param::new("fetchMode", ParamType::Int)
                .nullable()
                .with_default(ConstantValue::Null),
            Param::new("fetchModeArgs", ParamType::Mixed).variadic(),
        ],
        statement::query,
    )
    .tentative_return("PDOStatement|false"),
    MethodEntry::public_async(
        "quote",
        &[
            Param::new("string", ParamType::String),
            Param::new("type", ParamType::Int)
                .with_default(ConstantValue::Int(constants::PARAM_STR)),
        ],
        connection::quote,
    )
    .tentative_return("string|false"),
    MethodEntry::public_async("rollBack", &[], transactions::rollback).tentative_return("bool"),
    MethodEntry::public_async(
        "setAttribute",
        &[
            Param::new("attribute", ParamType::Int),
            Param::new("value", ParamType::Mixed),
        ],
        attributes::set,
    )
    .tentative_return("bool"),
    MethodEntry::public(
        "pgsqlCopyFromArray",
        &[
            Param::new("tableName", ParamType::String),
            Param::new("rows", ParamType::Iterable),
            Param::new("separator", ParamType::String).optional(),
            Param::new("nullAs", ParamType::String).optional(),
            Param::new("fields", ParamType::String)
                .nullable()
                .optional(),
        ],
        statement::unsupported_method,
    ),
    MethodEntry::public(
        "pgsqlCopyFromFile",
        &[
            Param::new("tableName", ParamType::String),
            Param::new("filename", ParamType::String),
            Param::new("separator", ParamType::String).optional(),
            Param::new("nullAs", ParamType::String).optional(),
            Param::new("fields", ParamType::String)
                .nullable()
                .optional(),
        ],
        statement::unsupported_method,
    ),
    MethodEntry::public(
        "pgsqlCopyToArray",
        &[
            Param::new("tableName", ParamType::String),
            Param::new("separator", ParamType::String).optional(),
            Param::new("nullAs", ParamType::String).optional(),
            Param::new("fields", ParamType::String)
                .nullable()
                .optional(),
        ],
        statement::unsupported_method,
    ),
    MethodEntry::public(
        "pgsqlCopyToFile",
        &[
            Param::new("tableName", ParamType::String),
            Param::new("filename", ParamType::String),
            Param::new("separator", ParamType::String).optional(),
            Param::new("nullAs", ParamType::String).optional(),
            Param::new("fields", ParamType::String)
                .nullable()
                .optional(),
        ],
        statement::unsupported_method,
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
    MethodEntry::public_async(
        "fetch",
        &[
            Param::new("mode", ParamType::Int).optional(),
            Param::new("cursorOrientation", ParamType::Int).optional(),
            Param::new("cursorOffset", ParamType::Int).optional(),
        ],
        crate::drivers::sqlite::fetch::fetch,
    ),
    MethodEntry::public_async(
        "fetchAll",
        &[
            Param::new("mode", ParamType::Int).optional(),
            Param::new("args", ParamType::Mixed).variadic(),
        ],
        crate::drivers::sqlite::fetch::all,
    ),
    MethodEntry::public_async(
        "fetchColumn",
        &[Param::new("column", ParamType::Int).optional()],
        crate::drivers::sqlite::fetch::column,
    ),
    MethodEntry::public(
        "setFetchMode",
        &[
            Param::new("mode", ParamType::Int),
            Param::new("args", ParamType::Mixed).variadic(),
        ],
        fetch::set_mode,
    ),
    MethodEntry::public_async("closeCursor", &[], crate::drivers::sqlite::fetch::close),
    MethodEntry::public("columnCount", &[], statement::column_count),
    MethodEntry::public("rowCount", &[], statement::row_count),
    MethodEntry::public("errorCode", &[], statement::error_code),
    MethodEntry::public("errorInfo", &[], statement::error_info),
    MethodEntry::public_async(
        "fetchObject",
        &[
            Param::new("class", ParamType::String).nullable().optional(),
            Param::new("constructorArgs", ParamType::Array).optional(),
        ],
        crate::drivers::sqlite::fetch::object,
    ),
    MethodEntry::public(
        "bindColumn",
        &[
            Param::new("column", ParamType::IntOrString),
            Param::new("var", ParamType::Mixed).by_ref(),
            Param::new("type", ParamType::Int).optional(),
            Param::new("maxLength", ParamType::Int).optional(),
            Param::new("driverOptions", ParamType::Mixed).optional(),
        ],
        statement::unsupported_method,
    ),
    MethodEntry::public_async(
        "getColumnMeta",
        &[Param::new("column", ParamType::Int)],
        crate::metadata::get,
    ),
    MethodEntry::public("nextRowset", &[], statement::unsupported_method),
    MethodEntry::public("getIterator", &[], statement::unsupported_method),
    MethodEntry::public("debugDumpParams", &[], statement::unsupported_method),
    MethodEntry::public(
        "getAttribute",
        &[Param::new("name", ParamType::Int)],
        crate::drivers::sqlite::methods::statement_attribute,
    ),
    MethodEntry::public(
        "setAttribute",
        &[
            Param::new("attribute", ParamType::Int),
            Param::new("value", ParamType::Mixed),
        ],
        statement::unsupported_method,
    ),
];

const SQLITE_METHODS: &[MethodEntry] = &[MethodEntry::public(
    "loadExtension",
    &[Param::new("name", ParamType::String)],
    crate::drivers::sqlite::methods::load,
)];
