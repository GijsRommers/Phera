//! PHP signatures for mysqli.

use super::super::dispatch;
use phera_extension_api::{ConstantValue, FunctionEntry, Param, ParamType};

use super::super::statement_dispatch;

pub const ENTRIES: &[FunctionEntry] = &[
    FunctionEntry::sync(
        "mysqli_thread_id",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::property::<17>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_prepare",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("query", ParamType::String),
        ],
        statement_dispatch::connection_proc::<{ statement_dispatch::PREPARE }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_execute_query",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("query", ParamType::String),
            Param::new("params", ParamType::Array)
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        statement_dispatch::connection_proc::<{ statement_dispatch::QUERY }>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_init",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        statement_dispatch::init_proc,
    ),
];
