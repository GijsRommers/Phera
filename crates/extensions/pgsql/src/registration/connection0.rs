//! PHP call signatures for connection.

use crate::{connection, information};
use phera_extension_api::{ConstantValue, FunctionEntry, Param, ParamType};

pub const ENTRIES: &[FunctionEntry] = &[
    FunctionEntry::sync(
        "pg_tty",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection"))
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        information::sync::<{ information::TTY }>,
    ),
    FunctionEntry::asynchronous(
        "pg_connection_reset",
        &[Param::new(
            "connection",
            ParamType::Class("PgSql\\Connection"),
        )],
        connection::reset,
    ),
    FunctionEntry::asynchronous(
        "pg_jit",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection"))
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        information::jit,
    ),
    FunctionEntry::sync(
        "pg_dbname",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection"))
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        information::sync::<0>,
    ),
    FunctionEntry::sync(
        "pg_host",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection"))
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        information::sync::<1>,
    ),
    FunctionEntry::sync(
        "pg_port",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection"))
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        information::sync::<2>,
    ),
    FunctionEntry::sync(
        "pg_options",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection"))
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        information::sync::<3>,
    ),
    FunctionEntry::asynchronous(
        "pg_version",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection"))
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        information::asynchronous::<{ information::VERSION }>,
    ),
    FunctionEntry::sync(
        "pg_client_encoding",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection"))
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        information::sync::<5>,
    ),
    FunctionEntry::sync(
        "pg_last_error",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection"))
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        information::sync::<6>,
    ),
    FunctionEntry::sync(
        "pg_get_pid",
        &[Param::new(
            "connection",
            ParamType::Class("PgSql\\Connection"),
        )],
        information::sync::<7>,
    ),
    FunctionEntry::sync(
        "pg_connection_status",
        &[Param::new(
            "connection",
            ParamType::Class("PgSql\\Connection"),
        )],
        information::sync::<8>,
    ),
    FunctionEntry::sync(
        "pg_transaction_status",
        &[Param::new(
            "connection",
            ParamType::Class("PgSql\\Connection"),
        )],
        information::sync::<9>,
    ),
    FunctionEntry::asynchronous(
        "pg_last_notice",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection")),
            Param::new("mode", ParamType::Int)
                .with_constant_default("PGSQL_NOTICE_LAST", ConstantValue::Int(1)),
        ],
        information::asynchronous::<10>,
    ),
    FunctionEntry::asynchronous(
        "pg_get_notify",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection")),
            Param::new("mode", ParamType::Int)
                .with_constant_default("PGSQL_ASSOC", ConstantValue::Int(1)),
        ],
        information::asynchronous::<11>,
    ),
    FunctionEntry::asynchronous(
        "pg_ping",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection"))
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        information::asynchronous::<12>,
    ),
    FunctionEntry::asynchronous(
        "pg_parameter_status",
        &[
            Param::new("connection", ParamType::Untyped),
            Param::new("name", ParamType::String).optional(),
        ],
        information::asynchronous::<13>,
    ),
    FunctionEntry::asynchronous(
        "pg_set_client_encoding",
        &[
            Param::new("connection", ParamType::Untyped),
            Param::new("encoding", ParamType::String).optional(),
        ],
        information::asynchronous::<14>,
    ),
    FunctionEntry::sync(
        "pg_connect_poll",
        &[Param::new(
            "connection",
            ParamType::Class("PgSql\\Connection"),
        )],
        information::sync::<15>,
    ),
    FunctionEntry::asynchronous(
        "pg_connect",
        &[
            Param::new("connection_string", ParamType::String),
            Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(0)),
        ],
        connection::connect::<false>,
    ),
    FunctionEntry::asynchronous(
        "pg_pconnect",
        &[
            Param::new("connection_string", ParamType::String),
            Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(0)),
        ],
        connection::connect::<true>,
    ),
    FunctionEntry::sync(
        "pg_close",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection"))
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        connection::close,
    ),
];
