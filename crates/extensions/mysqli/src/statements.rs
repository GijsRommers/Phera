//! `$stmt = $db->prepare('SELECT ?'); $stmt->execute(['value']);`.

pub mod binding;
pub mod methods;
pub mod properties;

use std::cell::RefCell;
use std::rc::Rc;

use mysql_async::{Params, Statement};
use phera_db_drivers::drivers::Rows;
use phera_db_drivers::drivers::mysql::Reply;
use phera_db_drivers::errors::DbError;
use phera_extension_api::{Context, NativeError, NativeResult, Object, PhpArray, Value};

use crate::{connection, errors, result};

pub type Handle = Rc<RefCell<State>>;
type Reference = Rc<RefCell<Value>>;

pub struct State {
    pub connection: connection::Handle,
    pub status: Status,
    pub parameters: Vec<(binding::Kind, Reference)>,
    pub output: Vec<Reference>,
    pub long_data: std::collections::HashMap<usize, Vec<u8>>,
    pub row: usize,
    pub rows: Option<Rows>,
    pub stored: bool,
    pub reply: Reply,
    pub error: Option<DbError>,
}

pub enum Status {
    Initialized,
    Open(Statement),
    Closed,
}

pub fn attach(object: &Object, connection: connection::Handle) -> Handle {
    let handle = Rc::new(RefCell::new(State {
        connection,
        status: Status::Initialized,
        parameters: Vec::new(),
        output: Vec::new(),
        long_data: Default::default(),
        row: 0,
        rows: None,
        stored: false,
        reply: Reply::default(),
        error: None,
    }));
    *object.native.borrow_mut() = Some(Box::new(handle.clone()));
    handle
}

pub fn init(context: &mut dyn Context, connection: connection::Handle) -> NativeResult {
    connection.borrow().lease()?;
    let value = context.create_object("mysqli_stmt", PhpArray::new())?;
    if let Value::Object(object) = &value {
        attach(object, connection);
    }
    Ok(value)
}

pub fn handle(object: &Object) -> Result<Handle, NativeError> {
    let state = object
        .native
        .borrow()
        .as_ref()
        .and_then(|data| data.downcast_ref::<Handle>())
        .cloned()
        .ok_or_else(not_initialized)?;
    if matches!(state.borrow().status, Status::Closed) {
        return Err(NativeError::error("mysqli_stmt object is already closed"));
    }
    Ok(state)
}

fn not_initialized() -> NativeError {
    NativeError::error("mysqli_stmt object is not fully initialized")
}

impl State {
    pub fn statement(&self) -> Result<&Statement, NativeError> {
        match &self.status {
            Status::Open(statement) => Ok(statement),
            _ => Err(not_initialized()),
        }
    }
}

pub async fn prepare(
    context: &mut dyn Context,
    handle: Handle,
    sql: Vec<u8>,
    call: &str,
) -> NativeResult {
    let connection = handle.borrow().connection.clone();
    let lease = connection.borrow().lease()?;
    let statement = connection::run(&lease, move |mysql| {
        Box::pin(async move { mysql.prepare_native(&sql).await })
    })
    .await;
    let mut state = handle.borrow_mut();
    match statement {
        Ok(statement) => {
            state.status = Status::Open(statement);
            state.error = None;
            state.rows = None;
            state.parameters.clear();
            state.output.clear();
            state.long_data.clear();
            connection.borrow_mut().error = None;
            Ok(Value::Bool(true))
        }
        Err(error) => {
            state.error = Some(error.clone());
            connection.borrow_mut().error = Some(error.clone());
            errors::report(context, &error, call, false)
        }
    }
}

pub async fn execute(
    context: &mut dyn Context,
    handle: Handle,
    list: Option<&PhpArray>,
    call: &str,
) -> NativeResult {
    let (statement, connection, params) = {
        let mut state = handle.borrow_mut();
        let statement = state.statement()?.clone();
        if list.is_none() && state.parameters.len() != usize::from(statement.num_params()) {
            let error = DbError {
                state: "HY000".into(),
                code: Some(2031),
                detail: "No data supplied for parameters in prepared statement".into(),
            };
            state.error = Some(error.clone());
            state.rows = None;
            return errors::report(context, &error, call, false);
        }
        let params = binding::parameters(context, &mut state, list, call)?;
        (statement, state.connection.clone(), params)
    };
    let lease = connection.borrow().lease()?;
    let outcome = connection::run(&lease, move |mysql| {
        Box::pin(async move {
            mysql
                .execute_native(&statement, Params::Positional(params))
                .await
        })
    })
    .await;
    let mut state = handle.borrow_mut();
    match outcome {
        Ok(outcome) => {
            connection.borrow_mut().observe(&outcome);
            state.reply = outcome.reply;
            state.error = None;
            state.row = 0;
            state.stored = false;
            state.rows = Some(result::decode(outcome.rows, true));
            Ok(Value::Bool(true))
        }
        Err(error) => {
            state.error = Some(error.clone());
            state.rows = None;
            errors::report(context, &error, call, false)
        }
    }
}
