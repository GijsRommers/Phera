//! PHP signatures for mysqli.

use super::super::dispatch;
use crate::connection;
use crate::connection::query;
use phera_extension_api::{ConstantValue, FunctionEntry, Param, ParamType};

pub const ENTRIES: &[FunctionEntry] = &[
    FunctionEntry::asynchronous(
        "mysqli_connect",
        &[
            Param::new("hostname", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
            Param::new("username", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
            Param::new("password", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null)
                .sensitive(),
            Param::new("database", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
            Param::new("port", ParamType::Int)
                .nullable()
                .with_default(ConstantValue::Null),
            Param::new("socket", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        connection::connect_function,
    ),
    FunctionEntry::asynchronous(
        "mysqli_real_connect",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("hostname", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
            Param::new("username", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
            Param::new("password", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null)
                .sensitive(),
            Param::new("database", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
            Param::new("port", ParamType::Int)
                .nullable()
                .with_default(ConstantValue::Null),
            Param::new("socket", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
            Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(0)),
        ],
        dispatch::connect_function,
    ),
    FunctionEntry::sync("mysqli_init", &[], connection::init),
    FunctionEntry::sync(
        "mysqli_options",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("option", ParamType::Int),
            Param::new("value", ParamType::Mixed),
        ],
        dispatch::local_function::<{ dispatch::OPTION }>,
    ),
    FunctionEntry::sync(
        "mysqli_set_opt",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("option", ParamType::Int),
            Param::new("value", ParamType::Mixed),
        ],
        dispatch::local_function::<{ dispatch::OPTION }>,
    ),
    FunctionEntry::sync(
        "mysqli_ssl_set",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("key", ParamType::String).nullable(),
            Param::new("certificate", ParamType::String).nullable(),
            Param::new("ca_certificate", ParamType::String).nullable(),
            Param::new("ca_path", ParamType::String).nullable(),
            Param::new("cipher_algos", ParamType::String).nullable(),
        ],
        dispatch::local_function::<{ dispatch::SSL_SET }>,
    ),
    FunctionEntry::sync(
        "mysqli_close",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::local_function::<{ dispatch::CLOSE }>,
    ),
    FunctionEntry::sync(
        "mysqli_character_set_name",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::local_function::<{ dispatch::CHARSET }>,
    ),
    FunctionEntry::sync(
        "mysqli_real_escape_string",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("string", ParamType::String),
        ],
        dispatch::local_function::<{ dispatch::ESCAPE }>,
    ),
    FunctionEntry::sync(
        "mysqli_escape_string",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("string", ParamType::String),
        ],
        dispatch::local_function::<{ dispatch::ESCAPE }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_query",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("query", ParamType::String),
            Param::new("result_mode", ParamType::Int)
                .with_constant_default("MYSQLI_STORE_RESULT", ConstantValue::Int(0)),
        ],
        dispatch::query_proc::<{ query::QUERY }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_real_query",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("query", ParamType::String),
        ],
        dispatch::query_proc::<{ query::REAL_QUERY }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_multi_query",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("query", ParamType::String),
        ],
        dispatch::query_proc::<{ query::MULTI_QUERY }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_next_result",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::query_proc::<{ query::NEXT }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_store_result",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("mode", ParamType::Int).with_default(ConstantValue::Int(0)),
        ],
        dispatch::pending_async_proc::<{ query::STORE }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_use_result",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::pending_async_proc::<{ query::USE }>,
    ),
];
