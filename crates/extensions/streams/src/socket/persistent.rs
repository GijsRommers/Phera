//! A worker can reuse an idle `pfsockopen()` connection on its next request.

use std::{
    cell::RefCell,
    io,
    path::PathBuf,
    rc::{Rc, Weak},
    sync::{Arc, Mutex, Weak as SharedWeak, atomic::Ordering},
    time::Duration,
};

use phera_extension_api::{Context, Value, value::Resource};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use tokio::{
    runtime::{Handle, Id},
    time::Instant,
};

use super::{
    Connection, Io,
    address::{Address, Source, Transport},
    client::transport_error,
    tls, transport,
};

const IDLE_LIMIT: usize = 64;
const IDLE_TIME: Duration = Duration::from_secs(60);

#[derive(PartialEq, Eq)]
pub(super) enum IdKind {
    Stream,
    HostPort(i64),
}

#[derive(PartialEq, Eq)]
pub(super) struct Key {
    pub application: Option<PathBuf>,
    pub id: IdKind,
    pub uri: Vec<u8>,
    pub source: Option<Source>,
    pub tls: Option<tls::Options>,
}

pub(super) struct Dial {
    pub key: Key,
    pub kind: Transport,
    pub deadline: Option<Duration>,
    pub asynchronous: bool,
}

impl Dial {
    pub async fn open(&self, warning: impl FnMut(String)) -> io::Result<Arc<Connection>> {
        self.establish(self.material().await?, warning).await
    }

    async fn material(&self) -> io::Result<Option<tls::Material>> {
        match &self.key.tls {
            Some(options) => tls::Material::load(options).await.map(Some),
            None => Ok(None),
        }
    }

    async fn establish(
        &self,
        material: Option<tls::Material>,
        warning: impl FnMut(String),
    ) -> io::Result<Arc<Connection>> {
        let (_, authority) = Transport::parse(&self.key.uri).map_err(transport_error)?;
        let address = Address::parse(self.kind, authority).map_err(transport_error)?;
        if self.deadline.is_some_and(|wait| wait.is_zero())
            && !self.kind.datagram()
            && !self.asynchronous
        {
            return Err(io::Error::from_raw_os_error(nix::libc::ETIMEDOUT));
        }
        let open = transport::connect(
            &address,
            self.key.source.as_ref(),
            self.asynchronous,
            warning,
        );
        let socket = match self.deadline {
            Some(duration) => tokio::time::timeout(duration, open)
                .await
                .map_err(|_| io::Error::from_raw_os_error(nix::libc::ETIMEDOUT))??,
            None => open.await?,
        };
        let _ = socket.peer.set(address.host.to_owned());
        if let (Transport::Tls(method), Some(options)) = (self.kind, &self.key.tls) {
            let io = Io {
                socket: socket.clone(),
                blocked: true,
                timeout: self.deadline,
                chunk: 8192,
            };
            tls::start(&io, options, options.method.unwrap_or(method) | 1, material).await?;
        }
        Ok(socket)
    }

    pub async fn lease(self: &Arc<Self>, warning: impl FnMut(String)) -> io::Result<Lease> {
        let pool = Pool::current();
        let material = self.material().await?;
        let stamp = material.as_ref().map(|material| material.stamp);
        let mut reused = None;
        while let Some(socket) = pool.take(&self.key, stamp) {
            if socket.clean().await {
                reused = Some(socket);
                break;
            }
        }
        let connection = match reused {
            Some(connection) => connection,
            None => self.establish(material, warning).await?,
        };
        connection.timed_out.store(false, Ordering::Relaxed);
        Ok(Lease {
            connection,
            pool: Some(ReturnTo {
                pool,
                dial: self.clone(),
                stamp,
            }),
        })
    }
}

#[derive(Default)]
pub(super) struct Resources(Vec<(Arc<Dial>, Weak<Resource>)>);

pub(super) fn fork_resources(
    state: &dyn std::any::Any,
    fork: &mut phera_extension_api::value::Fork<'_>,
) -> Option<Box<dyn std::any::Any>> {
    let mut copy = Resources::default();
    for (dial, resource) in &state.downcast_ref::<Resources>()?.0 {
        if let Some(resource) = resource.upgrade().filter(|r| !r.is_closed())
            && let Value::Resource(resource) = fork.value(&Value::Resource(resource))
        {
            copy.0.push((dial.clone(), Rc::downgrade(&resource)));
        }
    }
    Some(Box::new(copy))
}

pub(super) fn existing(context: &mut dyn Context, key: &Key) -> Option<Value> {
    let resources = &mut context.state().get_or_default::<Resources>().0;
    resources.retain(|(_, resource)| resource.upgrade().is_some_and(|r| !r.is_closed()));
    resources
        .iter()
        .find(|(dial, _)| &dial.key == key)
        .and_then(|(_, resource)| resource.upgrade())
        .map(Value::Resource)
}

pub(super) fn remember(context: &mut dyn Context, dial: Arc<Dial>, value: &Value) {
    if let Value::Resource(resource) = value {
        context
            .state()
            .get_or_default::<Resources>()
            .0
            .push((dial, Rc::downgrade(resource)));
    }
}

pub(super) struct Lease {
    pub connection: Arc<Connection>,
    pool: Option<ReturnTo>,
}

struct ReturnTo {
    pool: Arc<Pool>,
    dial: Arc<Dial>,
    stamp: Option<[u8; 32]>,
}

impl Lease {
    pub fn direct(connection: Arc<Connection>) -> Self {
        Self {
            connection,
            pool: None,
        }
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(ReturnTo { pool, dial, stamp }) = self.pool.take()
            && Arc::strong_count(&self.connection) == 1
            && !self.connection.retired.load(Ordering::Relaxed)
            && self.connection.unread() == 0
        {
            pool.put(dial, stamp, self.connection.clone());
        }
    }
}

struct Idle {
    dial: Arc<Dial>,
    stamp: Option<[u8; 32]>,
    connection: Arc<Connection>,
    at: Instant,
}

struct Pool {
    runtime: Id,
    idle: Mutex<Vec<Idle>>,
}

thread_local! {
    static POOLS: RefCell<Vec<SharedWeak<Pool>>> = const { RefCell::new(Vec::new()) };
}

impl Pool {
    fn current() -> Arc<Self> {
        let handle = Handle::current();
        let runtime = handle.id();
        POOLS.with_borrow_mut(|pools| {
            pools.retain(|pool| pool.strong_count() != 0);
            if let Some(pool) = pools
                .iter()
                .filter_map(SharedWeak::upgrade)
                .find(|pool| pool.runtime == runtime)
            {
                return pool;
            }
            let pool = Arc::new(Self {
                runtime,
                idle: Mutex::new(Vec::new()),
            });
            pools.push(Arc::downgrade(&pool));
            handle.spawn(pool.clone().expire());
            pool
        })
    }

    fn take(&self, key: &Key, stamp: Option<[u8; 32]>) -> Option<Arc<Connection>> {
        let mut idle = self.idle.lock().ok()?;
        idle.retain(|entry| {
            entry.at.elapsed() < IDLE_TIME && (&entry.dial.key != key || entry.stamp == stamp)
        });
        let position = idle.iter().position(|entry| &entry.dial.key == key)?;
        Some(idle.swap_remove(position).connection)
    }

    fn put(&self, dial: Arc<Dial>, stamp: Option<[u8; 32]>, connection: Arc<Connection>) {
        if let Ok(mut idle) = self.idle.lock() {
            if idle.len() >= IDLE_LIMIT {
                let oldest = idle.iter().enumerate().min_by_key(|(_, item)| item.at);
                if let Some((index, _)) = oldest {
                    idle.swap_remove(index);
                }
            }
            idle.push(Idle {
                dial,
                stamp,
                connection,
                at: Instant::now(),
            });
        }
    }

    async fn expire(self: Arc<Self>) {
        loop {
            tokio::time::sleep(IDLE_TIME).await;
            if let Ok(mut idle) = self.idle.lock() {
                idle.retain(|entry| entry.at.elapsed() < IDLE_TIME);
            }
        }
    }
}

impl Connection {
    async fn clean(&self) -> bool {
        if self.eof() || self.timed_out() || self.retired.load(Ordering::Relaxed) {
            return false;
        }
        let mut fd = [PollFd::new(self.descriptor(), PollFlags::IN)];
        if !poll(
            &mut fd,
            Some(&Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            }),
        )
        .is_ok_and(|ready| ready == 0)
        {
            return false;
        }
        tls::no_buffered_input(self).await
    }
}
