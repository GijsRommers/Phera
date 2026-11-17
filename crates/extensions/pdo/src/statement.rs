//! Prepared statements retain their PDO connection: `$stmt = $pdo->prepare('SELECT :id')`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value, args,
};

use crate::connection::{Session, session};
use crate::drivers::{Prepared, Rows};
use crate::errors::{DbError, ErrorState, unsupported};
use crate::fetch::FetchMode;
use crate::parameters::{self, Binding, ParameterType};
use crate::placeholders::Sql;
use crate::statement_execute::{execute_named, fail};

pub struct Statement {
    // Rust drops fields in declaration order. Queue statement cleanup before
    // releasing the session, whose final drop may immediately start pool reset.
    pub prepared: Prepared,
    pub connection: Rc<Session>,
    pub sql: Sql,
    pub bindings: HashMap<String, Binding>,
    pub rows: Rows,
    pub cursor: usize,
    pub fetch: FetchMode,
    pub column: usize,
    pub fetch_flags: i64,
    pub error: ErrorState,
}

pub fn state(this: Option<&Rc<Object>>) -> Result<Rc<RefCell<Statement>>, NativeError> {
    let object = this.ok_or_else(|| NativeError::error("PDOStatement object is missing"))?;
    object
        .native
        .borrow()
        .as_ref()
        .and_then(|native| native.downcast_ref::<Rc<RefCell<Statement>>>())
        .cloned()
        .ok_or_else(|| NativeError::error("PDO object is uninitialized"))
}

pub fn prepare<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move { create(context, this.as_ref(), &arguments, false).await })
}

pub fn query<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let result = create(context, this.as_ref(), &arguments, true).await?;
        let Value::Object(object) = &result else {
            return Ok(result);
        };
        let executed = execute_named(context, Some(object.clone()), Vec::new(), "PDO::query").await;
        if executed.is_err() || matches!(&executed, Ok(Value::Bool(false))) {
            let statement = state(Some(object))?;
            let statement = statement.borrow();
            *statement.connection.error.borrow_mut() = statement.error.clone();
            return executed;
        }
        Ok(result)
    })
}

async fn create(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
    query: bool,
) -> NativeResult {
    let connection = session(this)?;
    let source = args::string(arguments, 0)
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    let method = if query { "PDO::query" } else { "PDO::prepare" };
    if source.is_empty() {
        return Err(NativeError::value_error(format!(
            "{method}(): Argument #1 ($query) must not be empty"
        )));
    }
    if !query && args::array(arguments, 1).is_some_and(|options| !options.is_empty()) {
        return Err(unsupported("Statement driver options"));
    }
    let attributes = connection.attributes.borrow().clone();
    let configuration = if query {
        crate::fetch_options::validate(
            crate::fetch_options::Method::Query,
            arguments,
            attributes.fetch,
            0,
            0,
        )?
    } else {
        crate::fetch_options::Configuration {
            mode: attributes.fetch,
            column: 0,
            flags: 0,
        }
    };
    let sql = match Sql::parse(source.clone()) {
        Ok(sql) => sql,
        Err(error) => return connection.fail(context, error, method),
    };
    let prepared = if attributes.emulate {
        Prepared::Emulated
    } else if connection.name == crate::dsn::DriverName::Postgres {
        let number = connection.statement_sequence.get() + 1;
        connection.statement_sequence.set(number);
        Prepared::PostgresDeferred(number)
    } else {
        let result = connection.driver.lock().await.prepare(&sql).await;
        match result {
            Ok(prepared) => prepared,
            Err(error) => return connection.fail(context, error, method),
        }
    };
    let mut properties = PhpArray::new();
    properties.insert(ArrayKey::str("queryString"), Value::from(source));
    let result = context.create_object("PDOStatement", properties)?;
    let Value::Object(object) = &result else {
        return Err(NativeError::error(
            "PDOStatement allocation did not create an object",
        ));
    };
    let error = if connection.name == crate::dsn::DriverName::Postgres {
        connection
            .last_error
            .borrow()
            .clone()
            .map_or(ErrorState::Unused, ErrorState::UnusedDriver)
    } else {
        ErrorState::Unused
    };
    connection.success();
    *object.native.borrow_mut() = Some(Box::new(Rc::new(RefCell::new(Statement {
        connection,
        sql,
        prepared,
        bindings: HashMap::new(),
        rows: Rows::default(),
        cursor: 0,
        fetch: configuration.mode,
        column: configuration.column,
        fetch_flags: configuration.flags,
        error,
    }))));
    Ok(result)
}

pub fn bind_value(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    bind(context, this, arguments, "PDOStatement::bindValue")
}

pub fn bind_param(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    bind(context, this, arguments, "PDOStatement::bindParam")
}

fn bind(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
    method: &str,
) -> NativeResult {
    if args::int(arguments, 3).unwrap_or(0) != 0 || !args::value(arguments, 4).is_null() {
        return Err(unsupported("Output parameter bindings"));
    }
    let state = state(this)?;
    let key = parameters::key(args::value(arguments, 0), Some(method))?;
    let invalid = {
        let statement = state.borrow();
        if matches!(
            statement.prepared,
            Prepared::PostgresDeferred(_) | Prepared::Postgres(_, _)
        ) {
            let valid = crate::native_parameters::valid_key(&statement.sql, &key)
                || (statement.sql.parameters.is_empty() && key.parse::<usize>().is_ok());
            if valid {
                None
            } else {
                let detail = match args::value(arguments, 0) {
                    Value::String(_) => format!(":{key}"),
                    _ => "parameter was not defined".into(),
                };
                Some(DbError::parameter(&detail))
            }
        } else {
            None
        }
    };
    if let Some(error) = invalid {
        return fail(&state, context, error, method);
    }
    let kind = ParameterType::parse(args::int(arguments, 2).unwrap_or(2))?;
    let binding = Binding {
        value: args::value(arguments, 1).clone(),
        kind,
    };
    // Conversion errors are immediate for bindValue; references are read at execute time.
    if !binding.value.is_reference() {
        binding.read(context)?;
    }
    state.borrow_mut().bindings.insert(key, binding);
    Ok(Value::Bool(true))
}

pub fn row_count(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(Value::Int(state(this)?.borrow().rows.affected as i64))
}

pub fn column_count(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(Value::Int(state(this)?.borrow().rows.columns.len() as i64))
}

pub fn close(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let state = state(this)?;
    let mut statement = state.borrow_mut();
    statement.rows.values.clear();
    statement.cursor = 0;
    Ok(Value::Bool(true))
}

pub fn error_code(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(state(this)?.borrow().error.code())
}

pub fn error_info(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    state(this)?.borrow().error.info()
}

pub fn unsupported_method(
    _: &mut dyn Context,
    _: Option<&Rc<Object>>,
    _: &[Value],
) -> NativeResult {
    Err(unsupported("This PDOStatement operation"))
}
