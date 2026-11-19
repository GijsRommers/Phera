//! PHP call signatures for result.

use crate::result;
use phera_extension_api::{ConstantValue, FunctionEntry, Param, ParamType};

pub const ENTRIES: &[FunctionEntry] = &[
    FunctionEntry::sync(
        "pg_fetch_row",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("row", ParamType::Int)
                .nullable()
                .with_default(ConstantValue::Null),
            Param::new("mode", ParamType::Int)
                .with_constant_default("PGSQL_NUM", ConstantValue::Int(2)),
        ],
        result::execute::<0>,
    ),
    FunctionEntry::sync(
        "pg_fetch_assoc",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("row", ParamType::Int)
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        result::execute::<1>,
    ),
    FunctionEntry::sync(
        "pg_fetch_array",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("row", ParamType::Int)
                .nullable()
                .with_default(ConstantValue::Null),
            Param::new("mode", ParamType::Int)
                .with_constant_default("PGSQL_BOTH", ConstantValue::Int(3)),
        ],
        result::execute::<2>,
    ),
    FunctionEntry::sync(
        "pg_fetch_all",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("mode", ParamType::Int)
                .with_constant_default("PGSQL_ASSOC", ConstantValue::Int(1)),
        ],
        result::execute::<3>,
    ),
    FunctionEntry::sync(
        "pg_fetch_all_columns",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("field", ParamType::Int).with_default(ConstantValue::Int(0)),
        ],
        result::execute::<4>,
    ),
    FunctionEntry::sync(
        "pg_fetch_result",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("row", ParamType::Untyped),
            Param::new("field", ParamType::IntOrString).with_default(ConstantValue::Null),
        ],
        result::execute::<5>,
    ),
    FunctionEntry::sync(
        "pg_result_seek",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("row", ParamType::Int),
        ],
        result::execute::<6>,
    ),
    FunctionEntry::sync(
        "pg_num_rows",
        &[Param::new("result", ParamType::Class("PgSql\\Result"))],
        result::execute::<7>,
    ),
    FunctionEntry::sync(
        "pg_num_fields",
        &[Param::new("result", ParamType::Class("PgSql\\Result"))],
        result::execute::<8>,
    ),
    FunctionEntry::sync(
        "pg_affected_rows",
        &[Param::new("result", ParamType::Class("PgSql\\Result"))],
        result::execute::<9>,
    ),
    FunctionEntry::sync(
        "pg_free_result",
        &[Param::new("result", ParamType::Class("PgSql\\Result"))],
        result::execute::<10>,
    ),
    FunctionEntry::sync(
        "pg_result_status",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("mode", ParamType::Int)
                .with_constant_default("PGSQL_STATUS_LONG", ConstantValue::Int(1)),
        ],
        result::execute::<11>,
    ),
    FunctionEntry::sync(
        "pg_result_error",
        &[Param::new("result", ParamType::Class("PgSql\\Result"))],
        result::execute::<12>,
    ),
    FunctionEntry::sync(
        "pg_result_error_field",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("field_code", ParamType::Int),
        ],
        result::execute::<13>,
    ),
    FunctionEntry::sync(
        "pg_last_oid",
        &[Param::new("result", ParamType::Class("PgSql\\Result"))],
        result::execute::<14>,
    ),
    FunctionEntry::sync(
        "pg_field_prtlen",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("row", ParamType::Untyped),
            Param::new("field", ParamType::IntOrString).with_default(ConstantValue::Null),
        ],
        result::execute::<15>,
    ),
    FunctionEntry::sync(
        "pg_field_is_null",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("row", ParamType::Untyped),
            Param::new("field", ParamType::IntOrString).with_default(ConstantValue::Null),
        ],
        result::execute::<16>,
    ),
    FunctionEntry::asynchronous(
        "pg_fetch_object",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("row", ParamType::Int)
                .nullable()
                .with_default(ConstantValue::Null),
            Param::new("class", ParamType::String).with_default(ConstantValue::String("stdClass")),
            Param::new("constructor_args", ParamType::Array)
                .with_default(ConstantValue::EmptyArray),
        ],
        result::fetch_object,
    ),
    FunctionEntry::sync(
        "pg_field_name",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("field", ParamType::Int),
        ],
        result::fields::sync::<0>,
    ),
    FunctionEntry::sync(
        "pg_field_size",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("field", ParamType::Int),
        ],
        result::fields::sync::<1>,
    ),
];
