//! Procedural calls such as `mysqli_query($db, $sql)` share the method implementation.

use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Object, Value, args};
use std::rc::Rc;

use crate::connection::{self, commands, escape, properties, query};
use crate::{errors, options, result};

pub fn connect_method<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        connection::connect(
            context,
            connection::handle(this.as_ref())?,
            &args,
            "mysqli::real_connect",
        )
        .await
    })
}

pub fn connect_function(
    context: &mut dyn Context,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        connection::connect(
            context,
            connection::handle(args::object(&args, 0))?,
            args.get(1..).unwrap_or_default(),
            "mysqli_real_connect",
        )
        .await
    })
}

pub fn initialize(_: &mut dyn Context, object: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    connection::handle(object)?;
    Ok(Value::Null)
}

pub const OPTION: usize = 0;
pub const SSL_SET: usize = 1;
pub const CLOSE: usize = 2;
pub const CHARSET: usize = 3;
pub const ESCAPE: usize = 4;

pub fn local_method<const OP: usize>(
    c: &mut dyn Context,
    o: Option<&Rc<Object>>,
    a: &[Value],
) -> NativeResult {
    let handle = connection::handle(o)?;
    match OP {
        OPTION => commands::set_option(c, handle, a),
        SSL_SET => {
            options::ssl_set(&mut handle.borrow_mut().options.mysql, a);
            Ok(Value::Bool(true))
        }
        CLOSE => {
            handle.borrow_mut().close()?;
            Ok(Value::Bool(true))
        }
        CHARSET => {
            let state = handle.borrow();
            state.lease()?;
            Ok(Value::from(state.charset.clone().unwrap_or_default()))
        }
        _ => escape::escape(handle, args::bytes(a, 0)),
    }
}

pub fn local_function<const OP: usize>(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    local_method::<OP>(c, args::object(a, 0), a.get(1..).unwrap_or_default())
}

pub fn pending_method<const OP: usize>(
    _c: &mut dyn Context,
    o: Option<&Rc<Object>>,
    _: &[Value],
) -> NativeResult {
    query::more_results(connection::handle(o)?)
}

pub fn pending_function<const OP: usize>(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    pending_method::<OP>(c, args::object(a, 0), a.get(1..).unwrap_or_default())
}

pub fn pending_async<'a, const OP: usize>(
    c: &'a mut dyn Context,
    o: Option<Rc<Object>>,
    a: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let (o, _) = target(o.as_ref(), &a);
        query::result_operation::<OP>(c, connection::handle(o)?).await
    })
}

pub fn pending_async_proc<const OP: usize>(
    c: &mut dyn Context,
    a: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    pending_async::<OP>(c, None, a)
}

pub fn query_call<const OP: usize, const METHOD: bool>(
    c: &mut dyn Context,
    o: Option<Rc<Object>>,
    a: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let (o, a) = target(o.as_ref(), &a);
        let name = match (OP, METHOD) {
            (query::QUERY, true) => "mysqli::query",
            (query::QUERY, false) => "mysqli_query",
            (query::REAL_QUERY, true) => "mysqli::real_query",
            (query::REAL_QUERY, false) => "mysqli_real_query",
            (query::MULTI_QUERY, true) => "mysqli::multi_query",
            (query::MULTI_QUERY, false) => "mysqli_multi_query",
            (query::NEXT, true) => "mysqli::next_result",
            _ => "mysqli_next_result",
        };
        query::execute::<OP>(c, connection::handle(o)?, a, name).await
    })
}

pub fn query_proc<const OP: usize>(
    c: &mut dyn Context,
    a: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    query_call::<OP, false>(c, None, a)
}

pub fn command_call<const OP: usize, const METHOD: bool>(
    c: &mut dyn Context,
    o: Option<Rc<Object>>,
    a: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let (o, a) = target(o.as_ref(), &a);
        let suffix = [
            "select_db",
            "set_charset",
            "autocommit",
            "ping",
            "stat",
            "kill",
            "change_user",
            "begin_transaction",
            "commit",
            "rollback",
            "savepoint",
            "release_savepoint",
            "get_warnings",
            "get_charset",
        ]
        .get(OP)
        .copied()
        .unwrap_or_default();
        let name = format!("{}{suffix}", if METHOD { "mysqli::" } else { "mysqli_" });
        commands::execute::<OP>(c, connection::handle(o)?, a, &name).await
    })
}

pub fn command_proc<const OP: usize>(
    c: &mut dyn Context,
    a: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    command_call::<OP, false>(c, None, a)
}

pub(super) fn target<'a>(
    object: Option<&'a Rc<Object>>,
    arguments: &'a [Value],
) -> (Option<&'a Rc<Object>>, &'a [Value]) {
    if object.is_some() {
        (object, arguments)
    } else {
        (
            args::object(arguments, 0),
            arguments.get(1..).unwrap_or_default(),
        )
    }
}

pub fn property<const OP: usize>(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    let name = [
        "affected_rows",
        "insert_id",
        "errno",
        "error",
        "error_list",
        "sqlstate",
        "field_count",
        "info",
        "server_info",
        "server_version",
        "client_info",
        "client_version",
        "host_info",
        "protocol_version",
        "warning_count",
        "connect_errno",
        "connect_error",
        "thread_id",
    ]
    .get(OP)
    .copied()
    .unwrap_or_default();
    match name {
        "client_info" => Ok(Value::from(properties::CLIENT_INFO)),
        "client_version" => Ok(Value::Int(properties::CLIENT_VERSION)),
        "connect_errno" => errors::connect_errno(c, a),
        "connect_error" => errors::connect_error(c, a),
        _ => {
            let object =
                args::object(a, 0).ok_or_else(|| NativeError::error("mysqli object is missing"))?;
            properties::get(c, object, name)
        }
    }
}

pub fn server_info(c: &mut dyn Context, o: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    properties::get(
        c,
        o.ok_or_else(|| NativeError::error("mysqli object is missing"))?,
        "server_info",
    )
}

pub fn result_method<const OP: usize>(
    c: &mut dyn Context,
    o: Option<&Rc<Object>>,
    a: &[Value],
) -> NativeResult {
    result_dispatch::<OP>(c, o, a, true)
}

pub fn result_proc<const OP: usize>(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    result_dispatch::<OP>(c, args::object(a, 0), a.get(1..).unwrap_or_default(), false)
}

fn result_dispatch<const OP: usize>(
    c: &mut dyn Context,
    o: Option<&Rc<Object>>,
    a: &[Value],
    method: bool,
) -> NativeResult {
    let object = o.ok_or_else(|| NativeError::error("mysqli_result object is missing"))?;
    let call = result_name::<OP>(method);
    result::execute::<OP>(c, object, a, &call)
}

pub fn result_async<'a, const OP: usize>(
    c: &'a mut dyn Context,
    o: Option<Rc<Object>>,
    a: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let call = result_name::<OP>(o.is_some());
        let (o, a) = target(o.as_ref(), &a);
        let object = o.ok_or_else(|| NativeError::error("mysqli_result object is missing"))?;
        result::stream::execute::<OP>(c, object, a, &call).await
    })
}

pub fn result_async_proc<const OP: usize>(
    c: &mut dyn Context,
    a: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    result_async::<OP>(c, None, a)
}

fn result_name<const OP: usize>(method: bool) -> String {
    let names = [
        "free_result",
        "fetch_row",
        "fetch_assoc",
        "fetch_array",
        "fetch_all",
        "fetch_column",
        "data_seek",
        "field_seek",
        "field_tell",
        "fetch_field",
        "fetch_field_direct",
        "fetch_fields",
        "num_fields",
        "num_rows",
        "fetch_lengths",
    ];
    let name = names.get(OP).copied().unwrap_or_default();
    format!(
        "{}{name}",
        if method { "mysqli_result::" } else { "mysqli_" }
    )
}

pub fn fetch_object<'a>(
    c: &'a mut dyn Context,
    o: Option<Rc<Object>>,
    a: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let call = if o.is_some() {
            "mysqli_result::fetch_object"
        } else {
            "mysqli_fetch_object"
        };
        let (o, a) = target(o.as_ref(), &a);
        let object = o.ok_or_else(|| NativeError::error("mysqli_result object is missing"))?;
        result::fetch_object(c, object, a, call).await
    })
}

pub fn fetch_object_proc(c: &mut dyn Context, a: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    fetch_object(c, None, a)
}
