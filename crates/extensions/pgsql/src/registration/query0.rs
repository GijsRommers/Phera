//! PHP call signatures for query.

use crate::query;
use phera_extension_api::{ConstantValue, FunctionEntry, Param, ParamType};

pub const ENTRIES: &[FunctionEntry] = &[
    FunctionEntry::asynchronous(
        "pg_query",
        &[
            Param::new("connection", ParamType::Untyped),
            Param::new("query", ParamType::String).with_default(ConstantValue::Null),
        ],
        query::execute::<0, false>,
    ),
    FunctionEntry::asynchronous(
        "pg_send_query",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection")),
            Param::new("query", ParamType::String),
        ],
        query::execute::<0, true>,
    ),
    FunctionEntry::asynchronous(
        "pg_query_params",
        &[
            Param::new("connection", ParamType::Untyped),
            Param::new("query", ParamType::Untyped),
            Param::new("params", ParamType::Array).with_default(ConstantValue::Null),
        ],
        query::execute::<1, false>,
    ),
    FunctionEntry::asynchronous(
        "pg_send_query_params",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection")),
            Param::new("query", ParamType::String),
            Param::new("params", ParamType::Array),
        ],
        query::execute::<1, true>,
    ),
    FunctionEntry::asynchronous(
        "pg_prepare",
        &[
            Param::new("connection", ParamType::Untyped),
            Param::new("statement_name", ParamType::String),
            Param::new("query", ParamType::String).with_default(ConstantValue::Null),
        ],
        query::execute::<2, false>,
    ),
    FunctionEntry::asynchronous(
        "pg_send_prepare",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection")),
            Param::new("statement_name", ParamType::String),
            Param::new("query", ParamType::String),
        ],
        query::execute::<2, true>,
    ),
    FunctionEntry::asynchronous(
        "pg_execute",
        &[
            Param::new("connection", ParamType::Untyped),
            Param::new("statement_name", ParamType::Untyped),
            Param::new("params", ParamType::Array).with_default(ConstantValue::Null),
        ],
        query::execute::<3, false>,
    ),
    FunctionEntry::asynchronous(
        "pg_send_execute",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection")),
            Param::new("statement_name", ParamType::String),
            Param::new("params", ParamType::Array),
        ],
        query::execute::<3, true>,
    ),
    FunctionEntry::asynchronous(
        "pg_get_result",
        &[Param::new(
            "connection",
            ParamType::Class("PgSql\\Connection"),
        )],
        query::get,
    ),
    FunctionEntry::sync(
        "pg_connection_busy",
        &[Param::new(
            "connection",
            ParamType::Class("PgSql\\Connection"),
        )],
        query::busy,
    ),
    FunctionEntry::sync(
        "pg_consume_input",
        &[Param::new(
            "connection",
            ParamType::Class("PgSql\\Connection"),
        )],
        query::consume,
    ),
];
