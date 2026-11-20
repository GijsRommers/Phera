//! PHP signatures for mysqli.

use super::super::dispatch;
use phera_extension_api::{ConstantValue, FunctionEntry, Param, ParamType};

pub const ENTRIES: &[FunctionEntry] = &[
    FunctionEntry::sync(
        "mysqli_insert_id",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::property::<1>,
    ),
    FunctionEntry::sync(
        "mysqli_errno",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::property::<2>,
    ),
    FunctionEntry::sync(
        "mysqli_error",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::property::<3>,
    ),
    FunctionEntry::sync(
        "mysqli_error_list",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::property::<4>,
    ),
    FunctionEntry::sync(
        "mysqli_sqlstate",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::property::<5>,
    ),
    FunctionEntry::sync(
        "mysqli_field_count",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::property::<6>,
    ),
    FunctionEntry::sync(
        "mysqli_info",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::property::<7>,
    ),
    FunctionEntry::sync(
        "mysqli_get_server_info",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::property::<8>,
    ),
    FunctionEntry::sync(
        "mysqli_get_server_version",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::property::<9>,
    ),
    FunctionEntry::sync(
        "mysqli_get_client_info",
        &[Param::new("mysql", ParamType::Class("mysqli"))
            .nullable()
            .with_default(ConstantValue::Null)],
        dispatch::property::<10>,
    ),
    FunctionEntry::sync("mysqli_get_client_version", &[], dispatch::property::<11>),
    FunctionEntry::sync(
        "mysqli_get_host_info",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::property::<12>,
    ),
    FunctionEntry::sync(
        "mysqli_get_proto_info",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::property::<13>,
    ),
    FunctionEntry::sync(
        "mysqli_warning_count",
        &[Param::new("mysql", ParamType::Class("mysqli"))],
        dispatch::property::<14>,
    ),
    FunctionEntry::sync("mysqli_connect_errno", &[], dispatch::property::<15>),
    FunctionEntry::sync("mysqli_connect_error", &[], dispatch::property::<16>),
];
