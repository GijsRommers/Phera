//! PHP call signatures for result.

use crate::result;
use phera_extension_api::{ConstantValue, FunctionEntry, Param, ParamType};

pub const ENTRIES: &[FunctionEntry] = &[
    FunctionEntry::sync(
        "pg_field_type",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("field", ParamType::Int),
        ],
        result::fields::sync::<2>,
    ),
    FunctionEntry::sync(
        "pg_field_type_oid",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("field", ParamType::Int),
        ],
        result::fields::sync::<3>,
    ),
    FunctionEntry::sync(
        "pg_field_num",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("field", ParamType::String),
        ],
        result::fields::sync::<4>,
    ),
    FunctionEntry::asynchronous(
        "pg_field_table",
        &[
            Param::new("result", ParamType::Class("PgSql\\Result")),
            Param::new("field", ParamType::Int),
            Param::new("oid_only", ParamType::Bool).with_default(ConstantValue::Bool(false)),
        ],
        result::fields::table,
    ),
];
