//! PHP 8.5's `pg_exec($db, $sql)` is an alias of `pg_query($db, $sql)`.

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeFn, NativeResult, Registry, Value,
};

use crate::{information, query, result};

const RESULT_ALIASES: &[(&str, &str, NativeFn)] = &[
    (
        "pg_numrows",
        "pg_num_rows",
        NativeFn::Sync(cell::<0, { result::NUM_ROWS }>),
    ),
    (
        "pg_numfields",
        "pg_num_fields",
        NativeFn::Sync(cell::<1, { result::NUM_FIELDS }>),
    ),
    (
        "pg_cmdtuples",
        "pg_affected_rows",
        NativeFn::Sync(cell::<2, { result::AFFECTED }>),
    ),
    (
        "pg_fieldname",
        "pg_field_name",
        NativeFn::Sync(field::<3, { result::fields::NAME }>),
    ),
    (
        "pg_fieldsize",
        "pg_field_size",
        NativeFn::Sync(field::<4, { result::fields::SIZE }>),
    ),
    (
        "pg_fieldtype",
        "pg_field_type",
        NativeFn::Sync(field::<5, { result::fields::TYPE }>),
    ),
    (
        "pg_fieldnum",
        "pg_field_num",
        NativeFn::Sync(field::<6, { result::fields::NUM }>),
    ),
    (
        "pg_result",
        "pg_fetch_result",
        NativeFn::Sync(cell::<7, { result::VALUE }>),
    ),
    (
        "pg_freeresult",
        "pg_free_result",
        NativeFn::Sync(cell::<8, { result::FREE }>),
    ),
    (
        "pg_getlastoid",
        "pg_last_oid",
        NativeFn::Sync(cell::<9, { result::OID }>),
    ),
];
const CONNECTION_ALIASES: &[(&str, &str, NativeFn)] = &[
    ("pg_exec", "pg_query", NativeFn::Async(exec)),
    (
        "pg_errormessage",
        "pg_last_error",
        NativeFn::Sync(info::<1, { information::LAST_ERROR }>),
    ),
    (
        "pg_clientencoding",
        "pg_client_encoding",
        NativeFn::Sync(info::<2, { information::ENCODING }>),
    ),
    (
        "pg_setclientencoding",
        "pg_set_client_encoding",
        NativeFn::Async(set_encoding),
    ),
];

pub fn register(
    registry: &mut Registry,
    entries: impl Iterator<Item = &'static FunctionEntry> + Clone,
) {
    for &(name, canonical, handler) in RESULT_ALIASES.iter().chain(CONNECTION_ALIASES) {
        let entry = entries
            .clone()
            .find(|entry| entry.name == canonical)
            .expect("canonical alias target must be registered");
        registry.function(FunctionEntry {
            name,
            params: entry.params,
            handler,
            id: phera_extension_api::NativeId::NONE,
        });
    }
}

fn deprecate(c: &mut dyn Context, name: &str, canonical: &str) {
    c.deprecated(format!(
        "Function {name}() is deprecated since 8.0, use {canonical}() instead"
    ));
}

fn cell<const INDEX: usize, const OP: usize>(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (name, canonical, _) = RESULT_ALIASES[INDEX];
    deprecate(c, name, canonical);
    result::execute_named::<OP>(c, a, name)
}

fn field<const INDEX: usize, const OP: usize>(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (name, canonical, _) = RESULT_ALIASES[INDEX];
    deprecate(c, name, canonical);
    result::fields::sync_named::<OP>(a, name)
}

fn info<const INDEX: usize, const OP: usize>(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (name, canonical, _) = CONNECTION_ALIASES[INDEX];
    deprecate(c, name, canonical);
    information::sync_named::<OP>(c, a, name)
}

fn exec(c: &mut dyn Context, a: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    query::execute_named::<0, false>(c, a, "pg_exec")
}

fn set_encoding(c: &mut dyn Context, a: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    let (name, canonical, _) = CONNECTION_ALIASES[3];
    deprecate(c, name, canonical);
    information::asynchronous_named::<{ information::SET_ENCODING }>(c, a, name)
}
