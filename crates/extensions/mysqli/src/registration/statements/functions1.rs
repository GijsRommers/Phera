//! PHP signatures for mysqli_stmt.

use phera_extension_api::{FunctionEntry, Param, ParamType};

use super::super::statement_dispatch;

pub const ENTRIES: &[FunctionEntry] = &[
    FunctionEntry::sync(
        "mysqli_stmt_insert_id",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::property::<1>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_num_rows",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::property::<2>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_param_count",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::property::<3>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_field_count",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::property::<4>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_errno",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::property::<5>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_error",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::property::<6>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_error_list",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::property::<7>,
    ),
    FunctionEntry::sync(
        "mysqli_stmt_sqlstate",
        &[Param::new("statement", ParamType::Class("mysqli_stmt"))],
        statement_dispatch::property::<8>,
    ),
];
