//! PHP signatures for mysqli.

use super::super::dispatch;
use crate::connection;
use crate::connection::query;
use phera_extension_api::{ConstantValue, MethodEntry, Param, ParamType};

pub const ENTRIES: &[MethodEntry] = &[
    MethodEntry::public_async(
        "__construct",
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
        connection::construct,
    ),
    MethodEntry::public_async(
        "real_connect",
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
            Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(0)),
        ],
        dispatch::connect_method,
    )
    .tentative_return("bool"),
    MethodEntry::public("init", &[], dispatch::initialize),
    MethodEntry::public("get_server_info", &[], dispatch::server_info).tentative_return("string"),
    MethodEntry::public(
        "options",
        &[
            Param::new("option", ParamType::Int),
            Param::new("value", ParamType::Untyped),
        ],
        dispatch::local_method::<{ dispatch::OPTION }>,
    )
    .tentative_return("bool"),
    MethodEntry::public(
        "set_opt",
        &[
            Param::new("option", ParamType::Int),
            Param::new("value", ParamType::Untyped),
        ],
        dispatch::local_method::<{ dispatch::OPTION }>,
    )
    .tentative_return("bool"),
    MethodEntry::public(
        "ssl_set",
        &[
            Param::new("key", ParamType::String).nullable(),
            Param::new("certificate", ParamType::String).nullable(),
            Param::new("ca_certificate", ParamType::String).nullable(),
            Param::new("ca_path", ParamType::String).nullable(),
            Param::new("cipher_algos", ParamType::String).nullable(),
        ],
        dispatch::local_method::<{ dispatch::SSL_SET }>,
    )
    .tentative_return("true"),
    MethodEntry::public("close", &[], dispatch::local_method::<{ dispatch::CLOSE }>)
        .tentative_return("true"),
    MethodEntry::public(
        "character_set_name",
        &[],
        dispatch::local_method::<{ dispatch::CHARSET }>,
    )
    .tentative_return("string"),
    MethodEntry::public(
        "real_escape_string",
        &[Param::new("string", ParamType::String)],
        dispatch::local_method::<{ dispatch::ESCAPE }>,
    )
    .tentative_return("string"),
    MethodEntry::public(
        "escape_string",
        &[Param::new("string", ParamType::String)],
        dispatch::local_method::<{ dispatch::ESCAPE }>,
    )
    .tentative_return("string"),
    MethodEntry::public_async(
        "query",
        &[
            Param::new("query", ParamType::String),
            Param::new("result_mode", ParamType::Int)
                .with_constant_default("MYSQLI_STORE_RESULT", ConstantValue::Int(0)),
        ],
        dispatch::query_call::<{ query::QUERY }, true>,
    )
    .tentative_return("mysqli_result|bool"),
    MethodEntry::public_async(
        "real_query",
        &[Param::new("query", ParamType::String)],
        dispatch::query_call::<{ query::REAL_QUERY }, true>,
    )
    .tentative_return("bool"),
    MethodEntry::public_async(
        "multi_query",
        &[Param::new("query", ParamType::String)],
        dispatch::query_call::<{ query::MULTI_QUERY }, true>,
    )
    .tentative_return("bool"),
    MethodEntry::public_async(
        "next_result",
        &[],
        dispatch::query_call::<{ query::NEXT }, true>,
    )
    .tentative_return("bool"),
    MethodEntry::public_async(
        "store_result",
        &[Param::new("mode", ParamType::Int).with_default(ConstantValue::Int(0))],
        dispatch::pending_async::<{ query::STORE }>,
    )
    .tentative_return("mysqli_result|false"),
];
