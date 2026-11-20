//! `pg_lo_open($db, $oid, 'r+')` opens a large object in the current transaction.

use std::fmt::Write;

use phera_db_drivers::drivers::postgres::Postgres;
use phera_db_drivers::errors::DbError;
use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, PhpArray, Value, args, value::to_php_string,
};

use crate::connection::{self, Handle};

pub const CREATE: usize = 0;
pub const UNLINK: usize = 1;
pub const OPEN: usize = 2;
pub const CLOSE: usize = 3;
pub const READ: usize = 4;
pub const WRITE: usize = 5;
pub const READ_ALL: usize = 6;
pub const SEEK: usize = 7;
pub const TELL: usize = 8;
pub const TRUNCATE: usize = 9;

pub const NAMES: &[&str] = &[
    "pg_lo_create",
    "pg_lo_unlink",
    "pg_lo_open",
    "pg_lo_close",
    "pg_lo_read",
    "pg_lo_write",
    "pg_lo_read_all",
    "pg_lo_seek",
    "pg_lo_tell",
    "pg_lo_truncate",
];

pub const ALIASES: &[&str] = &[
    "pg_locreate",
    "pg_lounlink",
    "pg_loopen",
    "pg_loclose",
    "pg_loread",
    "pg_lowrite",
    "pg_loreadall",
];

const CHUNK: i64 = 8192;

#[derive(Clone)]
struct Lob {
    connection: Handle,
    descriptor: i64,
}

pub fn execute<const OP: usize, const ALIAS: bool>(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let canonical = NAMES[OP];
        let name = if ALIAS {
            let name = ALIASES[OP];
            context.deprecated(format!(
                "Function {name}() is deprecated since 8.0, use {canonical}() instead"
            ));
            name
        } else {
            canonical
        };
        match OP {
            CREATE | UNLINK | OPEN => connect_operation::<OP>(context, &arguments, name).await,
            _ => descriptor_operation::<OP>(context, &arguments, name).await,
        }
    })
}

async fn connect_operation<const OP: usize>(
    context: &mut dyn Context,
    arguments: &[Value],
    name: &str,
) -> NativeResult {
    let explicit = args::object(arguments, 0).is_some();
    let index = usize::from(explicit);
    let object = if explicit {
        args::value(arguments, 0)
    } else {
        &Value::Null
    };
    let state = connection::argument(context, object, name)?;
    let number = if OP == CREATE && arguments.len() == index {
        0
    } else {
        oid(args::value(arguments, index), OP == CREATE)?
    };
    let sql = match OP {
        CREATE => format!("lo_create({number}::oid)"),
        UNLINK => format!("lo_unlink({number}::oid)"),
        _ => {
            let mode = to_php_string(args::value(arguments, index + 1), context.diagnostics())?;
            let flags = match mode.as_bytes() {
                b"r" => 0x40000,
                b"w" => 0x20000,
                b"r+" | b"w+" => 0x60000,
                _ => {
                    return Err(NativeError::value_error(
                        "Mode must be one of 'r', 'r+', 'w', or 'w+'",
                    ));
                }
            };
            format!("lo_open({number}::oid, {flags})")
        }
    };
    match number_result(&state, sql).await? {
        Some(value) if OP == CREATE => Ok(Value::Int(value)),
        Some(_) if OP == UNLINK => Ok(Value::Bool(true)),
        Some(descriptor) => {
            let value = context.create_object("PgSql\\Lob", PhpArray::new())?;
            if let Value::Object(object) = &value {
                *object.native.borrow_mut() = Some(Box::new(Lob {
                    connection: state,
                    descriptor,
                }));
            }
            Ok(value)
        }
        None => {
            let detail = match OP {
                CREATE => "Unable to create PostgreSQL large object".into(),
                UNLINK => format!("Unable to delete PostgreSQL large object {number}"),
                _ => "Unable to open PostgreSQL large object".into(),
            };
            context.warning(format!("{name}(): {detail}"));
            Ok(Value::Bool(false))
        }
    }
}

pub(crate) fn oid(value: &Value, allow_zero: bool) -> Result<u32, NativeError> {
    let invalid = || NativeError::value_error("Invalid OID value passed");
    match &*value.deref() {
        Value::Int(value) if *value > 0 || (allow_zero && *value == 0) => Ok(*value as u32),
        Value::Int(_) => Err(invalid()),
        Value::String(value) => {
            let text = value.to_string_lossy();
            if text.is_empty() {
                return Ok(0);
            }
            text.trim_start()
                .parse::<i128>()
                .map(|number| number as u32)
                .map_err(|_| invalid())
        }
        value => Err(NativeError::type_error(format!(
            "OID value must be of type string|int, {} given",
            value.type_name()
        ))),
    }
}

async fn descriptor_operation<const OP: usize>(
    context: &mut dyn Context,
    arguments: &[Value],
    name: &str,
) -> NativeResult {
    let lob = args::object(arguments, 0)
        .and_then(|object| {
            object
                .native
                .borrow()
                .as_ref()?
                .downcast_ref::<Lob>()
                .cloned()
        })
        .ok_or_else(|| NativeError::error("PostgreSQL large object has already been closed"))?;
    let fd = lob.descriptor;
    let first = args::int(arguments, 1);
    let second = args::int(arguments, 2);
    let command = match OP {
        READ if first.is_some_and(|length| length < 0) => {
            return Err(NativeError::value_error(format!(
                "{name}(): Argument #2 ($length) must be greater or equal than 0"
            )));
        }
        READ => {
            return Ok(read(&lob, first.unwrap_or(CHUNK))
                .await?
                .map_or(Value::Bool(false), Value::from));
        }
        READ_ALL => {
            let mut written = 0i64;
            while let Some(bytes) = read(&lob, CHUNK).await? {
                if bytes.is_empty() {
                    break;
                }
                written = written.saturating_add(bytes.len() as i64);
                context.write(&bytes);
            }
            return Ok(Value::Int(written));
        }
        WRITE => {
            let bytes = args::bytes(arguments, 1);
            let size = second.unwrap_or(bytes.len() as i64);
            if size < 0 || size as usize > bytes.len() {
                let detail = if size < 0 {
                    "must be greater than or equal to 0"
                } else {
                    "must be less than or equal to the length of argument #2 ($buf)"
                };
                return Err(NativeError::value_error(format!(
                    "{name}(): Argument #3 ($length) {detail}"
                )));
            }
            let mut command = format!("lowrite({fd}, pg_catalog.decode('");
            for byte in &bytes[..size as usize] {
                write!(command, "{byte:02x}").expect("writing to a String cannot fail");
            }
            command.push_str("', 'hex'))");
            command
        }
        SEEK if !matches!(second, None | Some(0..=2)) => {
            return Err(NativeError::value_error(format!(
                "{name}(): Argument #3 ($whence) must be one of PGSQL_SEEK_SET, PGSQL_SEEK_CUR, or PGSQL_SEEK_END"
            )));
        }
        SEEK => format!(
            "lo_lseek64({fd}, {}::bigint, {})",
            first.unwrap_or_default(),
            second.unwrap_or(1)
        ),
        TELL => format!("lo_tell64({fd})"),
        TRUNCATE => format!("lo_truncate64({fd}, {}::bigint)", first.unwrap_or_default()),
        _ => format!("lo_close({fd})"),
    };
    let value = number_result(&lob.connection, command).await?;
    if OP == CLOSE && value.is_none() {
        context.warning(format!(
            "{name}(): Unable to close PostgreSQL large object descriptor {fd}"
        ));
    }
    Ok(match OP {
        WRITE => value.map_or(Value::Bool(false), Value::Int),
        TELL => Value::Int(value.unwrap_or(-1)),
        _ => Value::Bool(value.is_some()),
    })
}

async fn read(lob: &Lob, length: i64) -> Result<Option<Vec<u8>>, NativeError> {
    let Ok(length) = i32::try_from(length) else {
        return Ok(None);
    };
    let expression = format!(
        "encode(pg_catalog.loread({}, {length}), 'hex')",
        lob.descriptor
    );
    Ok(scalar(&lob.connection, expression).await?.map(|hex| {
        hex.as_chunks::<2>()
            .0
            .iter()
            .filter_map(|pair| {
                let high = char::from(pair[0]).to_digit(16)?;
                let low = char::from(pair[1]).to_digit(16)?;
                Some((high * 16 + low) as u8)
            })
            .collect()
    }))
}

async fn number_result(state: &Handle, expression: String) -> Result<Option<i64>, NativeError> {
    Ok(scalar(state, expression)
        .await?
        .and_then(|bytes| std::str::from_utf8(&bytes).ok()?.parse().ok()))
}

async fn scalar(state: &Handle, expression: String) -> Result<Option<Vec<u8>>, NativeError> {
    let lease = state.borrow().lease()?;
    let result = connection::run(&lease, move |driver| {
        Box::pin(scalar_query(driver, expression))
    })
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            state.borrow_mut().last_error = error.detail;
            Ok(None)
        }
    }
}

async fn scalar_query(
    driver: &mut Postgres,
    expression: String,
) -> Result<Option<Vec<u8>>, DbError> {
    let sql = format!("SELECT pg_catalog.{expression}");
    let Some(mut result) = driver.native_query(&sql).await?.pop() else {
        return Ok(None);
    };
    if let Some(error) = result.error {
        return Err(error);
    }
    Ok(result.values.pop().and_then(|mut row| row.pop().flatten()))
}
