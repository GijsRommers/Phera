//! PHP call signatures for commands.

use crate::{copy, dml, escape};
use phera_extension_api::{ConstantValue, FunctionEntry, Param, ParamType};

pub const ENTRIES: &[FunctionEntry] = &[
    FunctionEntry::asynchronous(
        "pg_escape_string",
        &[
            Param::new("connection", ParamType::Mixed),
            Param::new("string", ParamType::String).with_default(ConstantValue::Null),
        ],
        escape::escape::<0>,
    ),
    FunctionEntry::asynchronous(
        "pg_escape_literal",
        &[
            Param::new("connection", ParamType::Mixed),
            Param::new("string", ParamType::String).with_default(ConstantValue::Null),
        ],
        escape::escape::<1>,
    ),
    FunctionEntry::asynchronous(
        "pg_escape_identifier",
        &[
            Param::new("connection", ParamType::Mixed),
            Param::new("string", ParamType::String).with_default(ConstantValue::Null),
        ],
        escape::escape::<2>,
    ),
    FunctionEntry::asynchronous(
        "pg_escape_bytea",
        &[
            Param::new("connection", ParamType::Mixed),
            Param::new("string", ParamType::String).with_default(ConstantValue::Null),
        ],
        escape::escape::<3>,
    ),
    FunctionEntry::sync(
        "pg_unescape_bytea",
        &[Param::new("string", ParamType::String)],
        escape::unescape_bytea,
    ),
    FunctionEntry::asynchronous(
        "pg_copy_from",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection")),
            Param::new("table_name", ParamType::String),
            Param::new("rows", ParamType::ArrayOrObject),
            Param::new("separator", ParamType::String).with_default(ConstantValue::String("\t")),
            Param::new("null_as", ParamType::String).with_default(ConstantValue::String("\\\\N")),
        ],
        copy::copy::<true>,
    ),
    FunctionEntry::asynchronous(
        "pg_copy_to",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection")),
            Param::new("table_name", ParamType::String),
            Param::new("separator", ParamType::String).with_default(ConstantValue::String("\t")),
            Param::new("null_as", ParamType::String).with_default(ConstantValue::String("\\\\N")),
        ],
        copy::copy::<false>,
    ),
    FunctionEntry::asynchronous(
        "pg_meta_data",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection")),
            Param::new("table_name", ParamType::String),
            Param::new("extended", ParamType::Bool).with_default(ConstantValue::Bool(false)),
        ],
        dml::execute::<0>,
    ),
    FunctionEntry::asynchronous(
        "pg_convert",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection")),
            Param::new("table_name", ParamType::String),
            Param::new("values", ParamType::Array),
            Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(0)),
        ],
        dml::execute::<1>,
    ),
    FunctionEntry::asynchronous(
        "pg_insert",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection")),
            Param::new("table_name", ParamType::String),
            Param::new("values", ParamType::Array),
            Param::new("flags", ParamType::Int)
                .with_constant_default("PGSQL_DML_EXEC", ConstantValue::Int(512)),
        ],
        dml::execute::<2>,
    ),
    FunctionEntry::asynchronous(
        "pg_update",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection")),
            Param::new("table_name", ParamType::String),
            Param::new("values", ParamType::Array),
            Param::new("conditions", ParamType::Array),
            Param::new("flags", ParamType::Int)
                .with_constant_default("PGSQL_DML_EXEC", ConstantValue::Int(512)),
        ],
        dml::execute::<3>,
    ),
    FunctionEntry::asynchronous(
        "pg_delete",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection")),
            Param::new("table_name", ParamType::String),
            Param::new("conditions", ParamType::Array),
            Param::new("flags", ParamType::Int)
                .with_constant_default("PGSQL_DML_EXEC", ConstantValue::Int(512)),
        ],
        dml::execute::<4>,
    ),
    FunctionEntry::asynchronous(
        "pg_select",
        &[
            Param::new("connection", ParamType::Class("PgSql\\Connection")),
            Param::new("table_name", ParamType::String),
            Param::new("conditions", ParamType::Array).with_default(ConstantValue::EmptyArray),
            Param::new("flags", ParamType::Int)
                .with_constant_default("PGSQL_DML_EXEC", ConstantValue::Int(512)),
            Param::new("mode", ParamType::Int)
                .with_constant_default("PGSQL_ASSOC", ConstantValue::Int(1)),
        ],
        dml::execute::<5>,
    ),
];
