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

pub struct Statement {
    pub connection: Rc<Session>,
    pub sql: Sql,
    pub prepared: Prepared,
    pub bindings: HashMap<String, Binding>,
    pub rows: Rows,
    pub cursor: usize,
    pub fetch: FetchMode,
    pub column: usize,
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
    let fetch = if query {
        FetchMode::parse(args::int(arguments, 1).unwrap_or(0), attributes.fetch)?
    } else {
        attributes.fetch
    };
    let column = if query {
        args::int(arguments, 2).unwrap_or(0)
    } else {
        0
    };
    let column = usize::try_from(column)
        .map_err(|_| NativeError::value_error("Column index must be greater than or equal to 0"))?;
    let sql = match Sql::parse(source.clone()) {
        Ok(sql) => sql,
        Err(error) => return connection.fail(context, error, method),
    };
    let prepared = if attributes.emulate {
        Prepared::Emulated
    } else if connection.name == crate::dsn::DriverName::Postgres {
        Prepared::PostgresDeferred
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
        fetch,
        column,
        error,
    }))));
    Ok(result)
}

pub fn execute<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    execute_named(context, this, arguments, "PDOStatement::execute")
}

fn execute_named<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    method: &'static str,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let state = state(this.as_ref())?;
        if let Some(params) = arguments.first().filter(|value| !value.is_null()) {
            state.borrow_mut().bindings = parameters::execute_bindings(params)?;
        }
        let (connection, sql, prepared, values) = {
            let statement = state.borrow();
            let mut ordered = parameters::ordered(&statement.sql, &statement.bindings);
            if matches!(statement.prepared, Prepared::Mysql(_))
                && statement.sql.parameters.len() > statement.bindings.len()
            {
                ordered = Err(DbError::parameter(""));
            }
            let values = match ordered {
                Ok(bindings) => Ok(bindings
                    .into_iter()
                    .map(|b| b.read(context))
                    .collect::<Result<Vec<_>, _>>()?),
                Err(error) => Err(error),
            };
            (
                statement.connection.clone(),
                statement.sql.clone(),
                statement.prepared.clone(),
                values,
            )
        };
        let values = match values {
            Ok(values) => values,
            Err(error) => return fail(&state, context, error, method),
        };
        let result = connection
            .driver
            .lock()
            .await
            .execute(&sql, &prepared, &values)
            .await;
        match result {
            Ok(rows) => {
                let mut statement = state.borrow_mut();
                statement.rows = rows;
                statement.cursor = 0;
                statement.error = ErrorState::Success;
                Ok(Value::Bool(true))
            }
            Err(error) => fail(&state, context, error, method),
        }
    })
}

fn fail(
    state: &RefCell<Statement>,
    context: &mut dyn Context,
    error: DbError,
    method: &str,
) -> NativeResult {
    let mode = {
        let mut statement = state.borrow_mut();
        *statement.connection.last_error.borrow_mut() = Some(error.clone());
        statement.error = ErrorState::Failed(error.clone());
        statement.rows = Rows::default();
        statement.cursor = 0;
        statement.connection.attributes.borrow().errors
    };
    error.report(context, mode, method)
}

pub fn bind(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    if args::int(arguments, 3).unwrap_or(0) != 0 || !args::value(arguments, 4).is_null() {
        return Err(unsupported("Output parameter bindings"));
    }
    let state = state(this)?;
    let key = parameters::key(args::value(arguments, 0), false)?;
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
