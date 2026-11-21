//! `stream_socket_client('tcp://localhost:6379')` creates a request-owned stream.

mod address;
mod client;
mod datagram;
mod functions;
pub(crate) mod http;
mod io;
mod line;
mod persistent;
mod server;
mod slot;
mod tls;
mod transport;
mod wire;

use std::{
    collections::VecDeque,
    io as std_io,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use phera_extension_api::{Context, Value};
use socket2::Socket as Descriptor;
use tokio::sync::Mutex;

use self::{
    address::Transport,
    wire::{Reactor, Wire},
};
use crate::stream::{Backing, Stream, Wrapper};

pub(crate) use slot::{Socket, StreamIo};

pub(crate) fn register(registry: &mut phera_extension_api::Registry) {
    functions::register(registry);
    line::register(registry);
    registry.fork_state::<persistent::Resources>(persistent::fork_resources);
}

pub(crate) struct Connection {
    wire: Wire,
    transport: Transport,
    listener: bool,
    paired: bool,
    eof: AtomicBool,
    timed_out: AtomicBool,
    retired: AtomicBool,
    unread: AtomicUsize,
    input: Mutex<VecDeque<u8>>,
    secure: Mutex<tls::State>,
    peer: std::sync::OnceLock<String>,
}

impl Connection {
    fn new(
        descriptor: Descriptor,
        transport: Transport,
        listener: bool,
        paired: bool,
    ) -> std_io::Result<Arc<Self>> {
        Ok(Self::with_reactor(
            Reactor::new(descriptor, transport, listener)?,
            transport,
            listener,
            paired,
        ))
    }

    fn with_reactor(
        reactor: Reactor,
        transport: Transport,
        listener: bool,
        paired: bool,
    ) -> Arc<Self> {
        Arc::new(Self {
            wire: Wire(Arc::new(reactor)),
            transport,
            listener,
            paired,
            eof: AtomicBool::new(false),
            timed_out: AtomicBool::new(false),
            retired: AtomicBool::new(false),
            unread: AtomicUsize::new(0),
            input: Mutex::new(VecDeque::new()),
            secure: Mutex::new(tls::State::Plain),
            peer: std::sync::OnceLock::new(),
        })
    }

    pub(crate) fn descriptor(&self) -> &Reactor {
        self.wire.0.as_ref()
    }

    fn sockref(&self) -> socket2::SockRef<'_> {
        socket2::SockRef::from(self.descriptor())
    }

    pub(crate) fn stream_type(&self) -> &'static str {
        if self.paired {
            "generic_socket"
        } else {
            self.transport.stream_type()
        }
    }

    pub(crate) fn eof(&self) -> bool {
        self.eof.load(Ordering::Relaxed) && self.unread() == 0
    }

    pub(crate) fn timed_out(&self) -> bool {
        self.timed_out.load(Ordering::Relaxed)
    }

    pub(crate) fn unread(&self) -> usize {
        self.unread.load(Ordering::Relaxed)
    }
}

#[derive(Clone)]
pub(crate) struct Io {
    pub socket: Arc<Connection>,
    pub blocked: bool,
    pub timeout: Option<Duration>,
    pub chunk: usize,
}

fn wrap(context: &mut dyn Context, socket: Socket, uri: &[u8], attached: &Value) -> Value {
    let socket = Arc::new(socket);
    socket.set_timeout(default_timeout(context));
    let stream = Stream::new(Backing::Socket(socket), Wrapper::Socket, b"r+", uri);
    let value = crate::request::new_resource(context.state(), "stream", stream);
    if let (Value::Resource(stream), Value::Resource(attached)) = (&value, attached) {
        crate::request::files(context)
            .stream_contexts
            .insert(stream.id, attached.clone());
    }
    value
}

pub(crate) fn default_timeout(context: &dyn Context) -> Option<Duration> {
    let seconds = context
        .ini_get(b"default_socket_timeout")
        .as_ref()
        .and_then(|value| {
            std::str::from_utf8(value.as_bytes())
                .ok()?
                .parse::<i64>()
                .ok()
        })
        .unwrap_or(60);
    u64::try_from(seconds).ok().map(Duration::from_secs)
}
