//! A connection the boot made, in the requests that fork it.
//!
//! ```php
//! $db = new mysqli('127.0.0.1', 'app', 'secret', 'shop');
//! $db->set_charset('utf8mb4');
//! $db->query("SET SESSION sql_mode = 'TRADITIONAL'");
//! Phera\snapshot();
//! $db->query('SELECT 1');   // each request: a pooled session, set up again
//! ```
//!
//! A fork cannot share the boot's session: two requests would talk over one
//! socket. Instead each fork's copy connects again on its first use, with
//! the same settings, from the shared pool (the pool resets a session
//! before it lends it again), and replays what the boot changed about the
//! session: the statements of `set_charset()`, `select_db()`, `autocommit()`
//! and every `SET` or `USE` statement the boot ran, in order.
//!
//! What a replay cannot bring back is refused when the snapshot is taken
//! (see `phera_db_drivers::session_log`), and so are a result still being
//! read and a user changed with `change_user()`.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use phera_db_drivers::drivers::Driver;
use phera_db_drivers::dsn::Dsn;
use phera_db_drivers::errors::DbError;
use phera_db_drivers::pool::{self, Policy};
use phera_extension_api::Object;
use phera_io_pool::Lease;

use super::{Connection, Handle, Status, run};

/// What `connect()` was given: enough to connect again.
pub struct Origin {
    dsn: Dsn,
    user: String,
    password: String,
    policy: Policy,
}

impl Origin {
    pub fn new(dsn: Dsn, user: String, password: String, policy: Policy) -> Self {
        Origin {
            dsn,
            user,
            password,
            policy,
        }
    }

    /// The same connection for a fork: one per request, so it may come from
    /// the shared pool, which resets each session it lends again.
    fn forked(&self) -> Self {
        Origin {
            dsn: self.dsn.clone(),
            user: self.user.clone(),
            password: self.password.clone(),
            policy: match self.policy {
                Policy::Unpooled | Policy::Pooled => Policy::Pooled,
                Policy::Persistent => Policy::Persistent,
            },
        }
    }

    /// Connects, replays `statements`, and returns the lease with the new
    /// session's thread id.
    pub async fn reconnect(
        &self,
        statements: Vec<Vec<u8>>,
    ) -> Result<(Lease<Driver>, u32), DbError> {
        let lease = pool::acquire(&self.dsn, &self.user, &self.password, self.policy).await?;
        let thread_id = run(&lease, move |mysql| {
            Box::pin(async move {
                for statement in &statements {
                    for outcome in mysql.query_native(statement, false).await? {
                        outcome?;
                    }
                }
                Ok(mysql.thread_id())
            })
        })
        .await?;
        Ok((lease, thread_id))
    }
}

/// The fork hook of `mysqli`: see the module comment.
pub fn fork_connection(
    _object: &Object,
    state: &dyn Any,
    _fork: &mut phera_extension_api::value::Fork<'_>,
) -> Result<Box<dyn Any>, String> {
    let handle = state
        .downcast_ref::<Handle>()
        .ok_or("its built-in state is not initialized")?;
    let copy = handle.borrow().forked()?;
    Ok(Box::new(Rc::new(RefCell::new(copy))) as Box<dyn Any>)
}

impl Connection {
    fn forked(&self) -> Result<Connection, String> {
        let status = match &self.status {
            Status::Initialized => Status::Initialized,
            Status::Closed => Status::Closed,
            Status::Reconnect(origin) => Status::Reconnect(origin.clone()),
            Status::Open(lease) => {
                self.replayable(lease)?;
                let origin = self
                    .origin
                    .as_ref()
                    .ok_or("it was opened in a way a fork cannot repeat")?;
                Status::Reconnect(Rc::new(origin.forked()))
            }
        };
        Ok(Connection {
            status,
            options: self.options.clone(),
            charset: self.charset.clone(),
            native_numbers: self.native_numbers,
            error: self.error.clone(),
            reply: self.reply.clone(),
            field_count: self.field_count,
            thread_id: self.thread_id,
            version: self.version.clone(),
            host_info: self.host_info.clone(),
            backslashes: self.backslashes,
            pending: None,
            pending_stream: None,
            more: Default::default(),
            origin: self.origin.clone(),
            session: self.session.clone(),
        })
    }

    /// Whether a new session with the replayed statements is the same as
    /// this one; why not, if not.
    fn replayable(&self, lease: &Lease<Driver>) -> Result<(), String> {
        const ADVICE: &str = "; finish it before Phera\\snapshot(), or connect after the call";
        if self.waiting_for_result() {
            return Err(format!(
                "a result of this connection is still being read{ADVICE}"
            ));
        }
        if let Some(reason) = self.session.unreplayable() {
            return Err(format!(
                "{reason}, which a new session would not have{ADVICE}"
            ));
        }
        let inspected = lease.try_inspect(|driver| match driver {
            Driver::Mysql(mysql) => Some((mysql.transaction_state(), mysql.reusable())),
            _ => None,
        });
        match inspected.flatten() {
            None => Err(format!("this connection is busy with an operation{ADVICE}")),
            Some((Some(true), _)) => {
                Err(format!("a transaction is open on this connection{ADVICE}"))
            }
            Some((_, false)) => Err(format!(
                "this connection changed its user with change_user(){ADVICE}"
            )),
            Some(_) => Ok(()),
        }
    }
}
