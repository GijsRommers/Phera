//! A PDO and its statements keep their worker's connection alive together.

use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;

use tokio::runtime::Handle;
use tokio::sync::{Notify, oneshot};
use tokio::task::JoinHandle;

use super::socket::{Socket, closed};
use super::{Driver, Key, Worker};
use crate::errors::DbError;

/// A borrowed future for a driver operation on the PHP worker.
pub type Operation<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// Work explicitly submitted by `pg_send_query` can finish independently.
pub type BackgroundOperation<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone)]
pub struct Lease(Rc<Shared>);

struct Shared {
    socket: Cell<Slot>,
    owner: Option<(Arc<Worker>, Key)>,
    waiting: Cell<bool>,
    available: Notify,
    runtime: Handle,
}

enum Slot {
    Ready(Socket),
    Busy,
    Background(oneshot::Receiver<Socket>),
    Closed,
}

struct Checkout<'a> {
    shared: &'a Shared,
    socket: Option<Socket>,
    restore_slot: bool,
}

impl Lease {
    pub(super) fn new(socket: Socket, owner: Option<(Arc<Worker>, Key)>) -> Self {
        Self(Rc::new(Shared {
            socket: Cell::new(Slot::Ready(socket)),
            owner,
            waiting: Cell::new(false),
            available: Notify::new(),
            runtime: Handle::current(),
        }))
    }

    pub fn try_inspect<T>(&self, inspect: impl FnOnce(&Driver) -> T) -> Option<T> {
        let mut checkout = self.0.try_checkout()?;
        let result = checkout.socket.as_ref()?.driver().ok().map(inspect);
        checkout.restore();
        result
    }

    pub async fn run<T>(
        &self,
        operation: impl AsyncFnOnce(&mut Driver) -> Result<T, DbError>,
    ) -> Result<T, DbError> {
        let mut checkout = self.0.checkout().await?;
        let socket = checkout.socket.as_mut().ok_or_else(closed)?;
        let result = operation(socket.driver_mut()?).await;
        checkout.restore();
        result
    }

    pub fn start<T: Send + 'static>(
        &self,
        operation: impl for<'a> FnOnce(&'a mut Driver) -> BackgroundOperation<'a, Result<T, DbError>>
        + Send
        + 'static,
    ) -> Result<JoinHandle<Result<T, DbError>>, DbError> {
        let mut checkout = self.0.try_checkout().ok_or_else(closed)?;
        let mut socket = checkout.socket.take().ok_or_else(closed)?;
        let (sender, receiver) = oneshot::channel();
        checkout.replace(Slot::Background(receiver));
        Ok(self.0.runtime.spawn(async move {
            let result = operation(socket.driver_mut()?).await;
            let _ = sender.send(socket);
            result
        }))
    }
}

impl Shared {
    fn replace(&self, next: Slot) {
        self.socket.set(next);
        if self.waiting.replace(false) {
            self.available.notify_waiters();
        }
    }

    fn try_checkout(&self) -> Option<Checkout<'_>> {
        let slot = self.socket.replace(Slot::Busy);
        let socket = match slot {
            Slot::Ready(socket) => socket,
            Slot::Background(mut receiver) => match receiver.try_recv() {
                Ok(socket) => socket,
                Err(oneshot::error::TryRecvError::Empty) => {
                    self.socket.set(Slot::Background(receiver));
                    return None;
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    self.replace(Slot::Closed);
                    return None;
                }
            },
            slot => {
                self.socket.set(slot);
                return None;
            }
        };
        Some(Checkout::new(self, Some(socket)))
    }

    async fn checkout(&self) -> Result<Checkout<'_>, DbError> {
        loop {
            match self.socket.replace(Slot::Busy) {
                Slot::Ready(socket) => return Ok(Checkout::new(self, Some(socket))),
                Slot::Background(receiver) => {
                    let mut checkout = Checkout::new(self, None);
                    match receiver.await {
                        Ok(socket) => {
                            checkout.socket = Some(socket);
                            return Ok(checkout);
                        }
                        Err(_) => return Err(closed()),
                    }
                }
                Slot::Busy => {
                    self.waiting.set(true);
                    self.available.notified().await;
                }
                Slot::Closed => {
                    self.socket.set(Slot::Closed);
                    return Err(closed());
                }
            }
        }
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        let socket = self.socket.replace(Slot::Closed);
        let owner = self.owner.take();
        match (socket, owner) {
            (Slot::Ready(socket), Some((worker, key))) => {
                self.runtime
                    .spawn(async move { worker.recycle(socket, key).await });
            }
            (Slot::Background(receiver), Some((worker, key))) => {
                self.runtime.spawn(async move {
                    if let Ok(socket) = receiver.await {
                        worker.recycle(socket, key).await;
                    }
                });
            }
            _ => {}
        }
    }
}

impl<'a> Checkout<'a> {
    fn new(shared: &'a Shared, socket: Option<Socket>) -> Self {
        Self {
            shared,
            socket,
            restore_slot: true,
        }
    }

    fn restore(&mut self) {
        if let Some(socket) = self.socket.take() {
            self.replace(Slot::Ready(socket));
        }
    }

    fn replace(&mut self, slot: Slot) {
        self.shared.replace(slot);
        self.restore_slot = false;
    }
}

impl Drop for Checkout<'_> {
    fn drop(&mut self) {
        if self.restore_slot {
            // Dropping a driver future may leave part of a reply unread.
            self.shared.replace(Slot::Closed);
        }
    }
}
