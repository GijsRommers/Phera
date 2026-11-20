//! PHP signatures for mysqli_stmt.

use crate::statements::methods;
use phera_extension_api::{ConstantValue, MethodEntry, Param, ParamType};

use super::super::statement_dispatch;

pub const ENTRIES: &[MethodEntry] = &[
    MethodEntry::public_async("get_warnings", &[], statement_dispatch::warnings)
        .tentative_return("mysqli_warning|false"),
    MethodEntry::public("num_rows", &[], statement_dispatch::num_rows)
        .tentative_return("string|int"),
    MethodEntry::public_async(
        "__construct",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("query", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        statement_dispatch::construct,
    ),
    MethodEntry::public_async(
        "prepare",
        &[Param::new("query", ParamType::String)],
        statement_dispatch::execute::<{ statement_dispatch::PREPARE }, true>,
    )
    .tentative_return("bool"),
    MethodEntry::public_async(
        "execute",
        &[Param::new("params", ParamType::Array)
            .nullable()
            .with_default(ConstantValue::Null)],
        statement_dispatch::execute::<{ statement_dispatch::EXECUTE }, true>,
    )
    .tentative_return("bool"),
    MethodEntry::public(
        "bind_param",
        &[
            Param::new("types", ParamType::String),
            Param::new("vars", ParamType::Mixed).variadic().by_ref(),
        ],
        statement_dispatch::method::<{ methods::BIND_PARAM }>,
    )
    .tentative_return("bool"),
    MethodEntry::public(
        "bind_result",
        &[Param::new("vars", ParamType::Mixed).variadic().by_ref()],
        statement_dispatch::method::<{ methods::BIND_RESULT }>,
    )
    .tentative_return("bool"),
    MethodEntry::public(
        "fetch",
        &[],
        statement_dispatch::method::<{ methods::FETCH }>,
    )
    .tentative_return("?bool"),
    MethodEntry::public(
        "store_result",
        &[],
        statement_dispatch::method::<{ methods::STORE }>,
    )
    .tentative_return("bool"),
    MethodEntry::public(
        "get_result",
        &[],
        statement_dispatch::method::<{ methods::RESULT }>,
    )
    .tentative_return("mysqli_result|false"),
    MethodEntry::public(
        "result_metadata",
        &[],
        statement_dispatch::method::<{ methods::META }>,
    )
    .tentative_return("mysqli_result|false"),
    MethodEntry::public(
        "send_long_data",
        &[
            Param::new("param_num", ParamType::Int),
            Param::new("data", ParamType::String),
        ],
        statement_dispatch::method::<{ methods::LONG_DATA }>,
    )
    .tentative_return("bool"),
    MethodEntry::public(
        "reset",
        &[],
        statement_dispatch::method::<{ methods::RESET }>,
    )
    .tentative_return("bool"),
    MethodEntry::public(
        "close",
        &[],
        statement_dispatch::method::<{ methods::CLOSE }>,
    )
    .tentative_return("true"),
    MethodEntry::public(
        "free_result",
        &[],
        statement_dispatch::method::<{ methods::FREE }>,
    )
    .tentative_return("void"),
    MethodEntry::public(
        "data_seek",
        &[Param::new("offset", ParamType::Int)],
        statement_dispatch::method::<{ methods::SEEK }>,
    )
    .tentative_return("void"),
    MethodEntry::public(
        "more_results",
        &[],
        statement_dispatch::method::<{ methods::MORE }>,
    )
    .tentative_return("bool"),
    MethodEntry::public(
        "next_result",
        &[],
        statement_dispatch::method::<{ methods::NEXT }>,
    )
    .tentative_return("bool"),
];
