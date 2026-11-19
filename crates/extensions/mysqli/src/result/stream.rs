//! `mysqli_query($db, $sql, MYSQLI_USE_RESULT)` reads rows on demand.

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use phera_db_drivers::drivers::{Driver, Rows};
use phera_db_drivers::errors::DbError;
use phera_extension_api::{Context, NativeError, Object};
use phera_io_pool::Lease;

use super::{closed, decode, handle};
use crate::connection;

#[derive(Clone)]
pub struct Stream {
    connection: Weak<RefCell<connection::Connection>>,
    lease: Lease<Driver>,
    pub owner: Arc<()>,
    native: bool,
}

impl Stream {
    pub fn new(connection: &connection::Handle, lease: Lease<Driver>, native: bool) -> Self {
        Self {
            connection: Rc::downgrade(connection),
            lease,
            owner: Arc::new(()),
            native,
        }
    }

    pub async fn read(&self, all: bool) -> Result<(Rows, bool), DbError> {
        if self
            .connection
            .upgrade()
            .is_some_and(|state| !state.borrow().connected())
        {
            return Err(crate::errors::out_of_sync());
        }
        let owner = self.owner.clone();
        let response = connection::run(&self.lease, move |driver| {
            Box::pin(async move { driver.read_stream(&owner, all).await })
        })
        .await?;
        Ok(response.map_or((Rows::default(), true), |(rows, done)| {
            (decode(rows, self.native), done)
        }))
    }

    pub fn complete(&self, affected: u64) {
        if let Some(connection) = self.connection.upgrade() {
            let mut state = connection.borrow_mut();
            let matches = state
                .pending_stream
                .as_ref()
                .is_some_and(|pending| pending.ptr_eq(&Arc::downgrade(&self.owner)));
            if matches {
                state.pending_stream = None;
                state.reply.affected = affected;
            }
        }
    }

    async fn close(self) -> Result<(), DbError> {
        let owner = self.owner.clone();
        connection::run(&self.lease, move |driver| {
            Box::pin(async move {
                driver.close_stream(&owner).await;
                Ok(())
            })
        })
        .await?;
        self.complete(0);
        Ok(())
    }
}

pub async fn load(
    context: &mut dyn Context,
    object: &Object,
    all: bool,
    call: &str,
) -> Result<bool, NativeError> {
    let handle = handle(object)?;
    let stream = {
        let state = handle.borrow();
        state.as_ref().ok_or_else(closed)?.stream.clone()
    };
    let Some(stream) = stream else {
        return Ok(true);
    };
    let (rows, done) = match stream.read(all).await {
        Ok(batch) => batch,
        Err(error) => {
            crate::errors::report(context, &error, call, false)?;
            return Ok(false);
        }
    };
    let mut state = handle.borrow_mut();
    let result = state.as_mut().ok_or_else(closed)?;
    let valid = result
        .stream
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(&stream.owner, &current.owner));
    if !valid {
        return Err(NativeError::error(
            "mysqli_result cursor changed during fetch",
        ));
    }
    if let Some(count) = &mut result.stream_count {
        *count += rows.values.len();
    }
    result.rows.values = rows.values;
    result.position = 0;
    result.last = None;
    if done {
        stream.complete(0);
        result.stream = None;
    }
    Ok(true)
}

pub async fn free(object: &Object) -> Result<(), NativeError> {
    let result = handle(object)?.borrow_mut().take().ok_or_else(closed)?;
    if let Some(stream) = result.stream {
        stream
            .close()
            .await
            .map_err(|error| NativeError::error(error.detail))?;
    }
    Ok(())
}

/// Validate arguments before consuming any row on the wire.
pub async fn execute<const OP: usize>(
    context: &mut dyn phera_extension_api::Context,
    object: &Object,
    arguments: &[phera_extension_api::Value],
    call: &str,
) -> phera_extension_api::NativeResult {
    use super::{ALL, ARRAY, COLUMN, FREE, Fetch};
    use phera_extension_api::{Value, args};
    if OP == FREE {
        free(object).await?;
        return Ok(Value::Null);
    }
    let arg = if call.starts_with("mysqli_result::") {
        1
    } else {
        2
    };
    let index = args::int(arguments, 0).unwrap_or(if OP == ALL { 2 } else { 0 });
    if matches!(OP, ARRAY | ALL) {
        Fetch::parse(
            args::int(arguments, 0).unwrap_or(if OP == ALL { 2 } else { 3 }),
            call,
            arg,
        )?;
    }
    if OP == COLUMN {
        let result = handle(object)?;
        let state = result.borrow();
        let count = state.as_ref().ok_or_else(closed)?.rows.columns.len();
        if usize::try_from(index).is_err() || index as usize >= count {
            return Err(NativeError::value_error(format!(
                "{call}(): Argument #{arg} ($column) must be a valid column index"
            )));
        }
    }
    if !load(context, object, OP == ALL, call).await? {
        return Ok(Value::Bool(false));
    }
    super::execute::<OP>(context, object, arguments, call)
}
