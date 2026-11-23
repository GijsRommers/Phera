//! PHP signatures for mysqli_stmt.

use crate::statements::methods;
use phera_extension_api::{ConstantValue, FunctionEntry, Param, ParamType};

use super::super::statement_dispatch;

pub const ENTRIES: &[FunctionEntry] = &[
    FunctionEntry::asynchronous(
        "mysqli_stmt_prepare",
        &[
            Param::new("statement", ParamType::Class("mysqli_stmt")),
            Param::new("query", ParamType::String),
        ],
        statement_dispatch::execute_proc::<{ statement_dispatch::PREPARE }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_stmt_execute",
        &[
            Param::new("statement", ParamType::Class("mysqli_stmt")),
            Param::new("params", ParamType::Array)
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        statement_dispatch::execute_proc::<{ statement_dispatch::EXECUTE }>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_bind_param",
        &[
            Param::new("statement", ParamType::Class("mysqli_stmt")),
            Param::new("types", ParamType::String),
            Param::new("vars", ParamType::Mixed).variadic().by_ref(),
        ],
        statement_dispatch::procedure::<{ methods::BIND_PARAM }>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_bind_result",
        &[
            Param::new("statement", ParamType::Class("mysqli_stmt")),
            Param::new("vars", ParamType::Mixed).variadic().by_ref(),
        ],
        statement_dispatch::procedure::<{ methods::BIND_RESULT }>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_fetch",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::procedure::<{ methods::FETCH }>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_store_result",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::procedure::<{ methods::STORE }>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_get_result",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::procedure::<{ methods::RESULT }>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_result_metadata",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::procedure::<{ methods::META }>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_send_long_data",
        &[
            Param::new("statement", ParamType::Class("mysqli_stmt")),
            Param::new("param_num", ParamType::Int),
            Param::new("data", ParamType::String),
        ],
        statement_dispatch::procedure::<{ methods::LONG_DATA }>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_reset",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::procedure::<{ methods::RESET }>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_close",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::procedure::<{ methods::CLOSE }>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_free_result",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::procedure::<{ methods::FREE }>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_data_seek",
        &[
            Param::new("statement", ParamType::Class("mysqli_stmt")),
            Param::new("offset", ParamType::Int),
        ],
        statement_dispatch::procedure::<{ methods::SEEK }>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_more_results",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::procedure::<{ methods::MORE }>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_next_result",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::procedure::<{ methods::NEXT }>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_affected_rows",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::property::<0>,
    ),
];
