//! Shared async connection accounting for `$pdo = new PDO(...)` and `new Redis()`.
//! Protocol reset belongs to the driver; permits cover reset and close as well as use.

mod bucket;
mod runtime;

pub use bucket::{Bucket, Idle};
pub use runtime::IoRuntime;
use std::future::Future;
use std::time::Duration;

pub trait Connection: Send + 'static {
    type Error: Send;
    fn reusable(&self) -> bool;
    fn reset(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn close(self) -> impl Future<Output = ()> + Send;
}

#[derive(Clone, Copy)]
pub struct Options {
    pub size: usize,
    pub idle: Duration,
    pub lifetime: Duration,
    pub wait: Duration,
    pub io_threads: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            size: 20,
            idle: Duration::from_secs(60),
            lifetime: Duration::from_secs(1800),
            wait: Duration::from_secs(30),
            io_threads: 2,
        }
    }
}
