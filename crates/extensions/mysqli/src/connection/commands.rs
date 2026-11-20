//! Operations on the live MySQL session, such as `$db->set_charset('utf8mb4')`.

use phera_db_drivers::drivers::mysql::identifier;
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, PhpArray, Value, args};

use super::query::failure;
use super::{Handle, run};
use crate::options;

pub const SELECT_DB: usize = 0;
pub const SET_CHARSET: usize = 1;
pub const AUTOCOMMIT: usize = 2;
pub const PING: usize = 3;
pub const STAT: usize = 4;
pub const KILL: usize = 5;
pub const CHANGE_USER: usize = 6;
pub const BEGIN: usize = 7;
pub const COMMIT: usize = 8;
pub const ROLLBACK: usize = 9;
pub const SAVEPOINT: usize = 10;
pub const RELEASE: usize = 11;
pub const WARNINGS: usize = 12;
pub const GET_CHARSET: usize = 13;

pub async fn execute<const OP: usize>(
    context: &mut dyn Context,
    handle: Handle,
    arguments: &[Value],
    call: &str,
) -> NativeResult {
    let lease = handle.borrow().lease()?;
    let text = |index| args::string(arguments, index).map(|s| s.to_string_lossy());
    let name = text(0).unwrap_or_default();
    let password = text(1).unwrap_or_default();
    let database = text(2);
    let enabled = args::bool(arguments, 0).unwrap_or_default();
    let flags = args::int(arguments, 0).unwrap_or_default();
    let transaction_name = text(1);
    let is_method = call.contains("::");
    if !is_method && matches!(OP, PING | KILL) {
        deprecated_command(context, call, false, OP == PING);
    }
    if OP == KILL && flags <= 0 {
        let argument = if is_method { 1 } else { 2 };
        return Err(NativeError::value_error(format!(
            "{call}(): Argument #{argument} ($process_id) must be greater than 0"
        )));
    }
    if OP == WARNINGS {
        let count = handle.borrow().reply.warnings;
        return warnings(context, handle, count, call).await;
    }
    if handle.borrow().waiting_for_result() {
        return failure(
            context,
            &mut handle.borrow_mut(),
            crate::errors::out_of_sync(),
            call,
        );
    }
    let requested = name.clone();
    let answer = run(&lease, move |mysql| {
        Box::pin(async move {
            match OP {
                SELECT_DB => mysql.select_db(&name).await?,
                SET_CHARSET => mysql.set_charset(&name).await?,
                CHANGE_USER => mysql.change_user(name, password, database).await?,
                AUTOCOMMIT => mysql.set_autocommit(enabled).await?,
                PING => mysql.health().await?,
                STAT => return mysql.server_info().await.map(Answer::Text),
                GET_CHARSET => {
                    let query = b"SELECT cs.CHARACTER_SET_NAME, c.COLLATION_NAME, \
                        cs.MAXLEN, c.ID FROM information_schema.CHARACTER_SETS cs \
                        JOIN information_schema.COLLATIONS c USING (CHARACTER_SET_NAME) \
                        WHERE c.COLLATION_NAME = @@collation_connection";
                    return mysql.query_native(query, false).await.map(Answer::Results);
                }
                _ => {
                    let sql = match OP {
                        KILL => format!("KILL CONNECTION {flags}"),
                        SAVEPOINT => format!("SAVEPOINT {}", identifier(&name)),
                        RELEASE => format!("RELEASE SAVEPOINT {}", identifier(&name)),
                        _ => transaction(OP, flags, transaction_name.as_deref()),
                    };
                    return mysql
                        .query_native(sql.as_bytes(), false)
                        .await
                        .map(Answer::Results);
                }
            }
            Ok(Answer::Done)
        })
    })
    .await;
    let mut state = handle.borrow_mut();
    match answer {
        Ok(Answer::Done) => {
            state.error = None;
            if OP == SET_CHARSET {
                state.charset = Some(requested);
            }
            Ok(Value::Bool(true))
        }
        Ok(Answer::Text(text)) => Ok(Value::from(text)),
        Ok(Answer::Results(mut results)) => {
            let Some(Ok(outcome)) = results.pop() else {
                return Ok(Value::Bool(false));
            };
            if OP == GET_CHARSET {
                let rows = outcome
                    .rows
                    .into_php()
                    .map_err(|error| NativeError::error(error.detail))?;
                return charset_object(context, &rows.values);
            }
            state.observe(&outcome);
            Ok(Value::Bool(true))
        }
        Err(error) => failure(context, &mut state, error, call),
    }
}

pub(crate) async fn warnings(
    context: &mut dyn Context,
    handle: Handle,
    count: u16,
    call: &str,
) -> NativeResult {
    let lease = handle.borrow().lease()?;
    if count == 0 {
        return Ok(Value::Bool(false));
    }
    if handle.borrow().waiting_for_result() {
        return failure(
            context,
            &mut handle.borrow_mut(),
            crate::errors::out_of_sync(),
            call,
        );
    }
    let fetched = run(&lease, |driver| {
        Box::pin(async { driver.query_native(b"SHOW WARNINGS", false).await })
    })
    .await;
    match fetched {
        Ok(mut results) => {
            let Some(Ok(outcome)) = results.pop() else {
                return Ok(Value::Bool(false));
            };
            let rows = outcome
                .rows
                .into_php()
                .map_err(|cause| NativeError::error(cause.detail))?;
            warning(context, rows.values)
        }
        Err(error) => failure(context, &mut handle.borrow_mut(), error, call),
    }
}

pub(crate) fn deprecated_command(context: &mut dyn Context, call: &str, method: bool, ping: bool) {
    let (item, kind) = if method {
        ("Method", "method")
    } else {
        ("Function", "function")
    };
    let detail = if ping {
        format!(
            "because the reconnect feature has been removed in PHP 8.2 and this {kind} is now redundant"
        )
    } else {
        "use KILL CONNECTION/QUERY SQL statement instead".to_owned()
    };
    context.deprecated(format!("{item} {call}() is deprecated since 8.4, {detail}"));
}

enum Answer {
    Done,
    Text(String),
    Results(
        Vec<Result<phera_db_drivers::drivers::mysql::Outcome, phera_db_drivers::errors::DbError>>,
    ),
}

fn transaction(operation: usize, flags: i64, name: Option<&str>) -> String {
    let name = name.map_or_else(String::new, |name| {
        let name: String = name
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
            .collect();
        format!(" /*{name}*/")
    });
    if operation == BEGIN {
        let mut modes = Vec::new();
        if flags & 1 != 0 {
            modes.push("WITH CONSISTENT SNAPSHOT");
        }
        if flags & 2 != 0 {
            modes.push("READ WRITE");
        }
        if flags & 4 != 0 {
            modes.push("READ ONLY");
        }
        let mut sql = format!("START TRANSACTION{name}");
        if !modes.is_empty() {
            sql.push(' ');
            sql.push_str(&modes.join(", "));
        }
        return sql;
    }
    let command = if operation == COMMIT {
        "COMMIT"
    } else {
        "ROLLBACK"
    };
    let chain = match flags & 3 {
        1 => " AND CHAIN",
        2 => " AND NO CHAIN",
        _ => "",
    };
    let release = match flags & 12 {
        4 => " RELEASE",
        8 => " NO RELEASE",
        _ => "",
    };
    format!("{command}{name}{chain}{release}")
}

fn charset_object(context: &mut dyn Context, rows: &[Vec<Value>]) -> NativeResult {
    let Some(row) = rows.first() else {
        return Ok(Value::Null);
    };
    let mut props = PhpArray::new();
    for (name, value) in [
        ("charset", row.first().cloned().unwrap_or(Value::Null)),
        ("collation", row.get(1).cloned().unwrap_or(Value::Null)),
        ("dir", Value::from("")),
        ("min_length", Value::Int(1)),
        ("max_length", row.get(2).cloned().unwrap_or(Value::Null)),
        ("number", row.get(3).cloned().unwrap_or(Value::Int(0))),
        ("state", Value::Int(1)),
        ("comment", Value::from("")),
    ] {
        props.insert(ArrayKey::str(name), value);
    }
    context.create_object("stdClass", props)
}

fn warning(context: &mut dyn Context, rows: Vec<Vec<Value>>) -> NativeResult {
    let Some(first) = rows.first() else {
        return Ok(Value::Bool(false));
    };
    let mut props = PhpArray::new();
    props.insert(
        ArrayKey::str("message"),
        first.get(2).cloned().unwrap_or(Value::Null),
    );
    props.insert(ArrayKey::str("sqlstate"), Value::from("HY000"));
    props.insert(
        ArrayKey::str("errno"),
        first.get(1).cloned().unwrap_or(Value::Null),
    );
    let value = context.create_object("mysqli_warning", props)?;
    if let Value::Object(object) = &value {
        *object.native.borrow_mut() = Some(Box::new(
            rows.into_iter()
                .skip(1)
                .collect::<std::collections::VecDeque<_>>(),
        ));
    }
    Ok(value)
}

pub fn next_warning(
    _: &mut dyn Context,
    this: Option<&std::rc::Rc<phera_extension_api::Object>>,
    _: &[Value],
) -> NativeResult {
    let Some(object) = this else {
        return Ok(Value::Bool(false));
    };
    let mut native = object.native.borrow_mut();
    let row = native
        .as_mut()
        .and_then(|data| data.downcast_mut::<std::collections::VecDeque<Vec<Value>>>())
        .and_then(std::collections::VecDeque::pop_front);
    let Some(row) = row else {
        return Ok(Value::Bool(false));
    };
    object.set(
        ArrayKey::str("message"),
        row.get(2).cloned().unwrap_or(Value::Null),
    );
    object.set(
        ArrayKey::str("errno"),
        row.get(1).cloned().unwrap_or(Value::Null),
    );
    Ok(Value::Bool(true))
}

pub fn set_option(context: &mut dyn Context, handle: Handle, arguments: &[Value]) -> NativeResult {
    options::option(context, &mut handle.borrow_mut(), arguments)
}
