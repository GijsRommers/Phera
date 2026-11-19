//! PHP signatures for mysqli.

use phera_extension_api::{ConstantValue, MethodEntry, Param, ParamType};

use super::super::{dispatch, statement_dispatch};
use super::methods0;

pub const ENTRIES: &[MethodEntry] = &[
    MethodEntry::public_async(
        "connect",
        methods0::CONNECT.params,
        dispatch::connect_method::<false>,
    )
    .tentative_return("bool"),
    MethodEntry::public("get_client_info", &[], dispatch::client_info).tentative_return("string"),
    MethodEntry::public("thread_safe", &[], dispatch::thread_safe).tentative_return("bool"),
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
