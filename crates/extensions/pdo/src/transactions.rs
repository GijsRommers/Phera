//! `$pdo->beginTransaction(); ...; $pdo->rollBack();` waits without blocking the worker.

use std::rc::Rc;

use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Object, Value};

use crate::connection::session;

#[derive(Clone, Copy)]
enum Operation {
    Begin,
    Commit,
    Rollback,
}

fn run<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    operation: Operation,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let session = session(this.as_ref())?;
        let active = session.transaction.get();
        let (sql, method, next) = match operation {
            Operation::Begin => {
                if active {
                    return Err(NativeError::throw(
                        "PDOException",
                        "There is already an active transaction",
                    ));
                }
                ("BEGIN", "PDO::beginTransaction", true)
            }
            Operation::Commit | Operation::Rollback => {
                if !active {
                    return Err(NativeError::throw(
                        "PDOException",
                        "There is no active transaction",
                    ));
                }
                match operation {
                    Operation::Commit => ("COMMIT", "PDO::commit", false),
                    _ => ("ROLLBACK", "PDO::rollBack", false),
                }
            }
        };
        let result = session.driver.lock().await.direct(sql).await;
        match result {
            Ok(_) => {
                session.transaction.set(next);
                session.success();
                Ok(Value::Bool(true))
            }
            Err(error) => session.fail(context, error, method),
        }
    })
}

pub fn begin<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    run(context, this, Operation::Begin)
}

pub fn commit<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    run(context, this, Operation::Commit)
}

pub fn rollback<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    run(context, this, Operation::Rollback)
}

pub fn active(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(Value::Bool(session(this)?.transaction.get()))
}
