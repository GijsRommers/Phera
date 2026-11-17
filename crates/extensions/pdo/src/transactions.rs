//! `$pdo->beginTransaction(); ...; $pdo->rollBack();` waits without blocking the worker.

use std::rc::Rc;

use phera_extension_api::{Context, LocalFuture, NativeResult, Object, Value};

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
        let mut driver = session.driver.lock().await;
        let active = driver
            .transaction_state()
            .await
            .unwrap_or(session.transaction.get());
        let (sql, method, next) = match operation {
            Operation::Begin => {
                if active {
                    return Err(crate::errors::exception(
                        "There is already an active transaction",
                        None,
                        0,
                    ));
                }
                (
                    match session.attributes.borrow().transaction_mode {
                        1 => "BEGIN IMMEDIATE",
                        2 => "BEGIN EXCLUSIVE",
                        _ => "BEGIN",
                    },
                    "PDO::beginTransaction",
                    true,
                )
            }
            Operation::Commit | Operation::Rollback => {
                if !active {
                    return Err(crate::errors::exception(
                        "There is no active transaction",
                        None,
                        0,
                    ));
                }
                match operation {
                    Operation::Commit => ("COMMIT", "PDO::commit", false),
                    _ => ("ROLLBACK", "PDO::rollBack", false),
                }
            }
        };
        let result = driver.direct(sql).await;
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

pub fn active<'a>(
    _: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let session = session(this.as_ref())?;
        let active = session
            .driver
            .lock()
            .await
            .transaction_state()
            .await
            .unwrap_or(session.transaction.get());
        session.transaction.set(active);
        Ok(Value::Bool(active))
    })
}
