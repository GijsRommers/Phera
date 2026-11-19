//! PHP signatures: `$stmt->bindParam(param: ':id', var: $id, type: SQLITE3_INTEGER)`.
use crate::{connection, query, result, statement, utility};
use phera_extension_api::{ClassEntry, ConstantValue, MethodEntry, Param, ParamType, Registry};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        parent: Some("Exception"),
        ..ClassEntry::class("SQLite3Exception")
    });
    registry.class(ClassEntry {
        constants: crate::constants::CLASS,
        methods: DATABASE,
        ..ClassEntry::class("SQLite3")
    });
    registry.class(ClassEntry {
        methods: STATEMENT,
        constants: &[
            ("EXPLAIN_MODE_PREPARED", ConstantValue::Int(0)),
            ("EXPLAIN_MODE_EXPLAIN", ConstantValue::Int(1)),
            ("EXPLAIN_MODE_EXPLAIN_QUERY_PLAN", ConstantValue::Int(2)),
        ],
        ..ClassEntry::class("SQLite3Stmt")
    });
    registry.class(ClassEntry {
        methods: RESULT,
        ..ClassEntry::class("SQLite3Result")
    });
    for (name, value) in crate::constants::GLOBAL {
        registry.constant(name, ConstantValue::Int(*value));
    }
}

const DATABASE: &[MethodEntry] = &[
    MethodEntry::public_async(
        "backup",
        &[
            Param::new("destination", ParamType::Object),
            Param::new("sourceDatabase", ParamType::String).optional(),
            Param::new("destinationDatabase", ParamType::String).optional(),
        ],
        crate::backup::backup,
    ),
    MethodEntry::public_async(
        "__construct",
        &[
            Param::new("filename", ParamType::String),
            Param::new("flags", ParamType::Int).optional(),
            Param::new("encryptionKey", ParamType::String).optional(),
        ],
        connection::open,
    ),
    MethodEntry::public_async(
        "open",
        &[
            Param::new("filename", ParamType::String),
            Param::new("flags", ParamType::Int).optional(),
            Param::new("encryptionKey", ParamType::String).optional(),
        ],
        connection::open,
    ),
    MethodEntry::public_async("close", &[], connection::close),
    MethodEntry::public_async(
        "exec",
        &[Param::new("query", ParamType::String)],
        connection::exec,
    ),
    MethodEntry::public_async(
        "busyTimeout",
        &[Param::new("milliseconds", ParamType::Int)],
        connection::timeout,
    ),
    MethodEntry::public_async("lastInsertRowID", &[], connection::last_id),
    MethodEntry::public_async("changes", &[], connection::changes),
    MethodEntry::public_async(
        "prepare",
        &[Param::new("query", ParamType::String)],
        statement::prepare,
    ),
    MethodEntry::public_async(
        "query",
        &[Param::new("query", ParamType::String)],
        query::query,
    ),
    MethodEntry::public_async(
        "querySingle",
        &[
            Param::new("query", ParamType::String),
            Param::new("entireRow", ParamType::Bool).optional(),
        ],
        query::single,
    ),
    MethodEntry::public_async("version", &[], utility::version).static_(),
    MethodEntry::public(
        "escapeString",
        &[Param::new("string", ParamType::String)],
        utility::escape,
    )
    .static_(),
    MethodEntry::public("lastErrorCode", &[], utility::error_code),
    MethodEntry::public("lastExtendedErrorCode", &[], utility::extended_code),
    MethodEntry::public("lastErrorMsg", &[], utility::error_message),
    MethodEntry::public(
        "enableExceptions",
        &[Param::new("enable", ParamType::Bool).optional()],
        utility::exceptions,
    ),
    MethodEntry::public(
        "enableExtendedResultCodes",
        &[Param::new("enable", ParamType::Bool).optional()],
        utility::extended,
    ),
    MethodEntry::public(
        "loadExtension",
        &[Param::new("name", ParamType::String)],
        utility::load,
    ),
];

const STATEMENT: &[MethodEntry] = &[
    MethodEntry::public(
        "__construct",
        &[
            Param::new("sqlite3", ParamType::Class("SQLite3")),
            Param::new("query", ParamType::String),
        ],
        private_constructor,
    )
    .with_visibility(phera_extension_api::Visibility::Private),
    MethodEntry::public(
        "bindValue",
        &[
            Param::new("param", ParamType::IntOrString),
            Param::new("value", ParamType::Mixed),
            Param::new("type", ParamType::Int).optional(),
        ],
        statement::bind,
    ),
    MethodEntry::public(
        "bindParam",
        &[
            Param::new("param", ParamType::IntOrString),
            Param::new("var", ParamType::Mixed).by_ref(),
            Param::new("type", ParamType::Int).optional(),
        ],
        statement::bind,
    ),
    MethodEntry::public_async("clear", &[], statement::clear),
    MethodEntry::public_async("close", &[], statement::close),
    MethodEntry::public_async("execute", &[], statement::execute),
    MethodEntry::public_async("reset", &[], statement::reset),
    MethodEntry::public("paramCount", &[], crate::statement_info::count),
    MethodEntry::public("readOnly", &[], crate::statement_info::readonly),
    MethodEntry::public("busy", &[], crate::statement_info::busy),
    MethodEntry::public("explain", &[], crate::statement_info::explain),
    MethodEntry::public_async(
        "getSQL",
        &[Param::new("expand", ParamType::Bool).optional()],
        crate::statement_info::sql,
    ),
];

const RESULT: &[MethodEntry] = &[
    MethodEntry::public("__construct", &[], private_constructor)
        .with_visibility(phera_extension_api::Visibility::Private),
    MethodEntry::public("numColumns", &[], result::count),
    MethodEntry::public(
        "columnName",
        &[Param::new("column", ParamType::Int)],
        result::name,
    ),
    MethodEntry::public(
        "columnType",
        &[Param::new("column", ParamType::Int)],
        result::kind,
    ),
    MethodEntry::public_async(
        "fetchArray",
        &[Param::new("mode", ParamType::Int).optional()],
        result::fetch,
    ),
    MethodEntry::public_async(
        "fetchAll",
        &[Param::new("mode", ParamType::Int).optional()],
        result::all,
    ),
    MethodEntry::public_async("reset", &[], result::reset),
    MethodEntry::public_async("finalize", &[], result::finalize),
];

fn private_constructor(
    _: &mut dyn phera_extension_api::Context,
    _: Option<&std::rc::Rc<phera_extension_api::Object>>,
    _: &[phera_extension_api::Value],
) -> phera_extension_api::NativeResult {
    Ok(phera_extension_api::Value::Null)
}
