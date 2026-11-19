//! PHP declarations for `pg_lo_read($lob, 8192)` and the large-object API.

use crate::{lob, lob_file};
use phera_extension_api::{ConstantValue, FunctionEntry, Param, ParamType, Registry};

const LOB: Param = Param::new("lob", ParamType::Class("PgSql\\Lob"));
const CONNECTION: Param = Param::new("connection", ParamType::Untyped);
const OID: Param = Param::new("oid", ParamType::Untyped).optional();
const CREATE: &[Param] = &[CONNECTION.optional(), OID];
const UNLINK: &[Param] = &[CONNECTION, OID];
const OPEN: &[Param] = &[
    CONNECTION,
    OID,
    Param::new("mode", ParamType::String).optional(),
];
const HANDLE: &[Param] = &[LOB];
const READ: &[Param] = &[
    LOB,
    Param::new("length", ParamType::Int).with_default(ConstantValue::Int(8192)),
];
const WRITE: &[Param] = &[
    LOB,
    Param::new("data", ParamType::String),
    Param::new("length", ParamType::Int)
        .nullable()
        .with_default(ConstantValue::Null),
];
const SEEK: &[Param] = &[
    LOB,
    Param::new("offset", ParamType::Int),
    Param::new("whence", ParamType::Int).with_constant_default("SEEK_CUR", ConstantValue::Int(1)),
];
const TRUNCATE: &[Param] = &[LOB, Param::new("size", ParamType::Int)];
const IMPORT: &[Param] = &[
    CONNECTION,
    Param::new("filename", ParamType::Untyped).optional(),
    OID,
];
const EXPORT: &[Param] = &[
    CONNECTION,
    OID,
    Param::new("filename", ParamType::Untyped).optional(),
];

pub(super) fn register(registry: &mut Registry) {
    add_alias::<{ lob::CREATE }>(registry, CREATE, "string|int|false");
    add_alias::<{ lob::UNLINK }>(registry, UNLINK, "bool");
    add_alias::<{ lob::OPEN }>(registry, OPEN, "PgSql\\Lob|false");
    add_alias::<{ lob::CLOSE }>(registry, HANDLE, "bool");
    add_alias::<{ lob::READ }>(registry, READ, "string|false");
    add_alias::<{ lob::WRITE }>(registry, WRITE, "int|false");
    add_alias::<{ lob::READ_ALL }>(registry, HANDLE, "int");
    add::<{ lob::SEEK }, false>(registry, SEEK, "bool");
    add::<{ lob::TELL }, false>(registry, HANDLE, "int");
    add::<{ lob::TRUNCATE }, false>(registry, TRUNCATE, "bool");
    for (name, params, handler, returns) in [
        (
            "pg_lo_import",
            IMPORT,
            lob_file::transfer::<true, false> as _,
            "string|int|false",
        ),
        (
            "pg_loimport",
            IMPORT,
            lob_file::transfer::<true, true> as _,
            "string|int|false",
        ),
        (
            "pg_lo_export",
            EXPORT,
            lob_file::transfer::<false, false> as _,
            "bool",
        ),
        (
            "pg_loexport",
            EXPORT,
            lob_file::transfer::<false, true> as _,
            "bool",
        ),
    ] {
        registry.function(FunctionEntry::asynchronous(name, params, handler));
        registry.describe_function(name, returns, None);
    }
}

fn add_alias<const OP: usize>(
    registry: &mut Registry,
    params: &'static [Param],
    returns: &'static str,
) {
    add::<OP, false>(registry, params, returns);
    add::<OP, true>(registry, params, returns);
}

fn add<const OP: usize, const ALIAS: bool>(
    registry: &mut Registry,
    params: &'static [Param],
    returns: &'static str,
) {
    let name = if ALIAS {
        lob::ALIASES[OP]
    } else {
        lob::NAMES[OP]
    };
    registry.function(FunctionEntry::asynchronous(
        name,
        params,
        lob::execute::<OP, ALIAS>,
    ));
    registry.describe_function(name, returns, None);
}
