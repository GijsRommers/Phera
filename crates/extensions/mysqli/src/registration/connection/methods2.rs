//! PHP signatures for mysqli.

use phera_extension_api::{ConstantValue, MethodEntry, Param, ParamType};

use super::super::statement_dispatch;

pub const ENTRIES: &[MethodEntry] = &[
    MethodEntry::public_async(
        "prepare",
        &[Param::new("query", ParamType::String)],
        statement_dispatch::connection_call::<{ statement_dispatch::PREPARE }, true>,
    )
    .tentative_return("mysqli_stmt|false"),
    MethodEntry::public_async(
        "execute_query",
        &[
            Param::new("query", ParamType::String),
            Param::new("params", ParamType::Array)
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        statement_dispatch::connection_call::<{ statement_dispatch::QUERY }, true>,
    )
    .returning("mysqli_result|bool"),
    MethodEntry::public("stmt_init", &[], statement_dispatch::init_method)
        .tentative_return("mysqli_stmt|false"),
];
