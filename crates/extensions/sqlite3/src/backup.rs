//! `backup()` uses the shared engine's page snapshot; destination statements must be closed.

use crate::diagnostics::session;
use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Object, Value, args};
use std::rc::Rc;

pub fn backup<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let source = session(this.as_ref())?;
        let Value::Object(object) = args::value(&arguments, 0) else {
            unreachable!()
        };
        let target = session(Some(object)).map_err(|_| {
            NativeError::type_error(format!(
                "SQLite3::backup(): Argument #1 ($destination) must be of type SQLite3, {} given",
                object.class.name()
            ))
        })?;
        let source_name = args::string(&arguments, 1)
            .map(|s| s.to_string_lossy())
            .unwrap_or_else(|| "main".into());
        let target_name = args::string(&arguments, 2)
            .map(|s| s.to_string_lossy())
            .unwrap_or_else(|| "main".into());
        match source
            .database()?
            .backup(&target.database()?, source_name, target_name)
            .await
        {
            Ok(()) => Ok(Value::Bool(true)),
            Err(error) => source.fail(context, error, "SQLite3::backup", "Backup failed: "),
        }
    })
}
