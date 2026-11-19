//! PHP signatures for mysqli.

use super::super::dispatch;
use crate::connection::{commands, query};
use phera_extension_api::{ConstantValue, MethodEntry, Param, ParamType};

pub const ENTRIES: &[MethodEntry] = &[
    MethodEntry::public(
        "use_result",
        &[],
        dispatch::pending_method::<{ query::USE }>,
    )
    .tentative_return("mysqli_result|false"),
    MethodEntry::public(
        "more_results",
        &[],
        dispatch::pending_method::<{ query::MORE }>,
    )
    .tentative_return("bool"),
    MethodEntry::public_async(
        "select_db",
        &[Param::new("database", ParamType::String)],
        dispatch::command_call::<{ commands::SELECT_DB }, true>,
    )
    .tentative_return("bool"),
    MethodEntry::public_async(
        "set_charset",
        &[Param::new("charset", ParamType::String)],
        dispatch::command_call::<{ commands::SET_CHARSET }, true>,
    )
    .tentative_return("bool"),
    MethodEntry::public_async(
        "autocommit",
        &[Param::new("enable", ParamType::Bool)],
        dispatch::command_call::<{ commands::AUTOCOMMIT }, true>,
    )
    .tentative_return("bool"),
    MethodEntry::public_async(
        "ping",
        &[],
        dispatch::command_call::<{ commands::PING }, true>,
    )
    .tentative_return("bool"),
    MethodEntry::public_async(
        "stat",
        &[],
        dispatch::command_call::<{ commands::STAT }, true>,
    )
    .tentative_return("string|false"),
    MethodEntry::public_async(
        "kill",
        &[Param::new("process_id", ParamType::Int)],
        dispatch::command_call::<{ commands::KILL }, true>,
    )
    .tentative_return("bool"),
    MethodEntry::public_async(
        "change_user",
        &[
            Param::new("username", ParamType::String),
            Param::new("password", ParamType::String).sensitive(),
            Param::new("database", ParamType::String).nullable(),
        ],
        dispatch::command_call::<{ commands::CHANGE_USER }, true>,
    )
    .tentative_return("bool"),
    MethodEntry::public_async(
        "begin_transaction",
        &[
            Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(0)),
            Param::new("name", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        dispatch::command_call::<{ commands::BEGIN }, true>,
    )
    .tentative_return("bool"),
    MethodEntry::public_async(
        "commit",
        &[
            Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(0)),
            Param::new("name", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        dispatch::command_call::<{ commands::COMMIT }, true>,
    )
    .tentative_return("bool"),
    MethodEntry::public_async(
        "rollback",
        &[
            Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(0)),
            Param::new("name", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        dispatch::command_call::<{ commands::ROLLBACK }, true>,
    )
    .tentative_return("bool"),
    MethodEntry::public_async(
        "savepoint",
        &[Param::new("name", ParamType::String)],
        dispatch::command_call::<{ commands::SAVEPOINT }, true>,
    )
    .tentative_return("bool"),
    MethodEntry::public_async(
        "release_savepoint",
        &[Param::new("name", ParamType::String)],
        dispatch::command_call::<{ commands::RELEASE }, true>,
    )
    .tentative_return("bool"),
    MethodEntry::public_async(
        "get_warnings",
        &[],
        dispatch::command_call::<{ commands::WARNINGS }, true>,
    )
    .tentative_return("mysqli_warning|false"),
    MethodEntry::public_async(
        "get_charset",
        &[],
        dispatch::command_call::<{ commands::GET_CHARSET }, true>,
    )
    .tentative_return("?object"),
];
