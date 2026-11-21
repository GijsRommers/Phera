//! A boot's `pfsockopen()` handle dials when a fork first needs its socket.

use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use phera_extension_api::value::Resource;
use tokio::sync::OnceCell;

use crate::{
    resource::with_stream,
    stream::{Backing, Stream},
};

use super::{
    Connection, Io,
    persistent::{Dial, Lease},
};

pub struct Socket {
    dial: Option<Arc<Dial>>,
    lease: OnceCell<Lease>,
    timeout: AtomicU64,
}

impl Socket {
    pub(super) fn live(socket: Arc<Connection>) -> Self {
        Self::with_lease(Lease::direct(socket))
    }

    pub(super) fn with_lease(lease: Lease) -> Self {
        Self {
            dial: None,
            lease: OnceCell::new_with(Some(lease)),
            timeout: AtomicU64::new(u64::MAX),
        }
    }

    pub(super) fn deferred(dial: Arc<Dial>) -> Self {
        Self {
            dial: Some(dial),
            lease: OnceCell::new(),
            timeout: AtomicU64::new(u64::MAX),
        }
    }

    pub(crate) fn fork(&self) -> Result<Self, String> {
        let Some(dial) = self.dial.clone().filter(|_| !self.lease.initialized()) else {
            return Err("an open stream socket belongs to the boot".into());
        };
        let socket = Self::deferred(dial);
        socket.set_timeout(self.timeout());
        Ok(socket)
    }

    pub(crate) fn ready(&self) -> Option<&Connection> {
        self.lease.get().map(|lease| lease.connection.as_ref())
    }

    pub(crate) async fn connect(&self) -> io::Result<Arc<Connection>> {
        let lease = self
            .lease
            .get_or_try_init(|| async {
                let dial = self.dial.as_ref().ok_or(io::ErrorKind::NotConnected)?;
                dial.lease(|_| {}).await
            })
            .await?;
        Ok(lease.connection.clone())
    }

    pub(crate) fn set_timeout(&self, timeout: Option<Duration>) {
        let nanos = timeout.map_or(u64::MAX, |duration| {
            duration.as_nanos().min(u128::from(u64::MAX - 1)) as u64
        });
        self.timeout.store(nanos, Ordering::Relaxed);
    }

    fn timeout(&self) -> Option<Duration> {
        let nanos = self.timeout.load(Ordering::Relaxed);
        (nanos != u64::MAX).then(|| Duration::from_nanos(nanos))
    }

    pub(crate) fn retire(&self) {
        if let Some(connection) = self.ready() {
            connection.retired.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn timed_out(&self) -> bool {
        self.ready().is_some_and(Connection::timed_out)
    }

    pub(crate) fn eof(&self) -> bool {
        self.ready().is_some_and(Connection::eof)
    }

    pub(crate) fn unread(&self) -> usize {
        self.ready().map_or(0, Connection::unread)
    }

    pub(crate) fn stream_type(&self) -> &'static str {
        self.ready().map_or_else(
            || {
                self.dial
                    .as_ref()
                    .map_or("tcp_socket/ssl", |dial| dial.kind.stream_type())
            },
            Connection::stream_type,
        )
    }

    pub(crate) fn crypto(&self) -> Option<phera_extension_api::Value> {
        super::tls::metadata(self.ready()?)
    }
}

pub(crate) struct StreamIo {
    socket: Arc<Socket>,
    blocked: bool,
    chunk: usize,
}

impl StreamIo {
    pub(crate) fn of(stream: &Stream) -> Option<Self> {
        let Backing::Socket(socket) = &stream.backing else {
            return None;
        };
        Some(Self {
            socket: socket.clone(),
            blocked: stream.blocked,
            chunk: stream.chunk_size,
        })
    }

    pub(crate) fn for_resource(resource: &Resource) -> Option<Self> {
        with_stream(resource, |stream| Self::of(stream))
            .ok()
            .flatten()
    }

    pub(super) async fn resolve(&self) -> io::Result<Io> {
        Ok(Io {
            socket: self.socket.connect().await?,
            blocked: self.blocked,
            timeout: self.socket.timeout(),
            chunk: self.chunk,
        })
    }

    pub(crate) async fn read(
        self,
        length: Option<u64>,
        ending: Option<&[u8]>,
    ) -> io::Result<Vec<u8>> {
        self.resolve().await?.read(length, ending).await
    }

    pub(crate) async fn write(self, bytes: &[u8]) -> io::Result<usize> {
        self.resolve().await?.write(bytes).await
    }
}
