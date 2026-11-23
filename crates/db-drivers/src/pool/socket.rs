//! An open database socket and the admission unit it holds until it closes.

use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use super::admission::Permit;
use crate::drivers::{Driver, connection_error};
use crate::errors::DbError;

pub(super) struct Socket {
    driver: Option<Box<Driver>>,
    pub(super) born: Instant,
    permit: Option<Permit>,
    runtime: Handle,
}

impl Socket {
    pub(super) fn new(driver: Driver, permit: Option<Permit>) -> Self {
        Self {
            driver: Some(Box::new(driver)),
            born: Instant::now(),
            permit,
            runtime: Handle::current(),
        }
    }

    pub(super) fn driver(&self) -> Result<&Driver, DbError> {
        self.driver.as_deref().ok_or_else(closed)
    }

    pub(super) fn driver_mut(&mut self) -> Result<&mut Driver, DbError> {
        self.driver.as_deref_mut().ok_or_else(closed)
    }

    pub(super) async fn close(mut self) {
        if let Some(closing) = self.start_close() {
            let _ = closing.await;
        }
    }

    fn start_close(&mut self) -> Option<JoinHandle<()>> {
        let driver = self.driver.take()?;
        let permit = self.permit.take();
        Some(self.runtime.spawn(async move {
            driver.close().await;
            drop(permit);
        }))
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        self.start_close();
    }
}

pub(super) fn closed() -> DbError {
    connection_error("the database connection has been closed")
}
