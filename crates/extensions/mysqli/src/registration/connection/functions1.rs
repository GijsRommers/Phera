//! PHP signatures for mysqli.

use super::super::dispatch;
use crate::connection::{commands, query};
use phera_extension_api::{ConstantValue, FunctionEntry, Param, ParamType};

pub const ENTRIES: &[FunctionEntry] = &[
    FunctionEntry::sync(
        "mysqli_more_results",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::pending_function::<{ query::MORE }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_select_db",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("database", ParamType::String),
        ],
        dispatch::command_proc::<{ commands::SELECT_DB }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_set_charset",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("charset", ParamType::String),
        ],
        dispatch::command_proc::<{ commands::SET_CHARSET }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_autocommit",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("enable", ParamType::Bool),
        ],
        dispatch::command_proc::<{ commands::AUTOCOMMIT }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_ping",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::command_proc::<{ commands::PING }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_stat",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::command_proc::<{ commands::STAT }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_kill",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("process_id", ParamType::Int),
        ],
        dispatch::command_proc::<{ commands::KILL }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_change_user",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("username", ParamType::String),
            Param::new("password", ParamType::String).sensitive(),
            Param::new("database", ParamType::String).nullable(),
        ],
        dispatch::command_proc::<{ commands::CHANGE_USER }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_begin_transaction",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(0)),
            Param::new("name", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        dispatch::command_proc::<{ commands::BEGIN }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_commit",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(0)),
            Param::new("name", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        dispatch::command_proc::<{ commands::COMMIT }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_rollback",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(0)),
            Param::new("name", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        dispatch::command_proc::<{ commands::ROLLBACK }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_savepoint",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("name", ParamType::String),
        ],
        dispatch::command_proc::<{ commands::SAVEPOINT }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_release_savepoint",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("name", ParamType::String),
        ],
        dispatch::command_proc::<{ commands::RELEASE }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_get_warnings",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::command_proc::<{ commands::WARNINGS }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_get_charset",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::command_proc::<{ commands::GET_CHARSET }>,
    ),
    FunctionEntry::sync(
        "mysqli_affected_rows",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::property::<0>,
    ),
];
