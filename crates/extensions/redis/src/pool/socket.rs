//! `$redis->connect('tls://redis.example')` opens an encrypted pooled socket.

use std::sync::Arc;
use std::time::Duration;

use futures_util::future::select_ok;
use redis::aio::MultiplexedConnection;
use redis::{
    AsyncConnectionConfig, Client, ConnectionAddr, ErrorKind, IntoConnectionInfo,
    RedisConnectionInfo, RedisError, RedisResult, Value,
};
use rustls::pki_types::ServerName;
use tokio::net::{TcpStream, lookup_host};
use tokio::task::AbortHandle;
use tokio_rustls::TlsConnector;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub timeout_micros: u64,
    pub persistent_id: Option<String>,
}

impl Endpoint {
    fn client(&self, settings: RedisConnectionInfo) -> RedisResult<Client> {
        let addr = if self.host.starts_with('/')
            || self.host.starts_with("unix://")
            || self.host.starts_with("file://")
        {
            ConnectionAddr::Unix(
                self.host
                    .trim_start_matches("unix://")
                    .trim_start_matches("file://")
                    .into(),
            )
        } else {
            ConnectionAddr::Tcp(self.host.trim_start_matches("tcp://").to_owned(), self.port)
        };
        let info = ("127.0.0.1", 6379).into_connection_info()?;
        Client::open(info.set_addr(addr).set_redis_settings(settings))
    }

    pub async fn open(
        &self,
        settings: RedisConnectionInfo,
        config: AsyncConnectionConfig,
        timeout: Option<Duration>,
    ) -> RedisResult<Socket> {
        if let Some(host) = self.host.strip_prefix("tls://") {
            let open = open_tls(host, self.port, &settings, config);
            return match timeout {
                Some(duration) => tokio::time::timeout(duration, open)
                    .await
                    .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))?,
                None => open.await,
            };
        }
        let connection = self
            .client(settings)?
            .get_multiplexed_async_connection_with_config(&config.set_connection_timeout(timeout))
            .await?;
        Ok(Socket {
            connection,
            reusable: true,
            _driver: None,
        })
    }
}

async fn open_tls(
    host: &str,
    port: u16,
    settings: &RedisConnectionInfo,
    config: AsyncConnectionConfig,
) -> RedisResult<Socket> {
    let mut addresses = lookup_host((host, port)).await?.peekable();
    if addresses.peek().is_none() {
        return Err((ErrorKind::InvalidClientConfig, "No address found for host").into());
    }
    let tls = phera_io_pool::tls::webpki_client().map_err(super::error)?;
    let name = ServerName::try_from(host.to_owned()).map_err(super::error)?;
    let connector = TlsConnector::from(tls);
    // As with redis-rs, try every address and keep the first completed TLS handshake.
    let (stream, _) = select_ok(addresses.map(|address| {
        let connector = connector.clone();
        let name = name.clone();
        Box::pin(async move {
            connector
                .connect(name, TcpStream::connect(address).await?)
                .await
        })
    }))
    .await?;
    let (connection, driver) =
        MultiplexedConnection::new_with_config(settings, stream, config).await?;
    Ok(Socket {
        connection,
        reusable: true,
        _driver: Some(Arc::new(Driver(tokio::spawn(driver).abort_handle()))),
    })
}

// With a custom stream, redis-rs leaves task ownership to the caller. Abort on
// the last drop even if an outstanding Redis command is waiting for a reply.
struct Driver(AbortHandle);

impl Drop for Driver {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Clone)]
pub struct Socket {
    pub connection: MultiplexedConnection,
    /// Cleared after an I/O error, or by `Lease::retire`; the pool then
    /// closes the socket instead of lending it again.
    pub reusable: bool,
    _driver: Option<Arc<Driver>>,
}

/// A failed handshake is tried this many more times, after a short pause.
const HANDSHAKE_RETRIES: u32 = 2;
const HANDSHAKE_PAUSE: Duration = Duration::from_millis(2);

impl Socket {
    pub async fn connect(endpoint: Endpoint) -> RedisResult<Self> {
        let config = AsyncConnectionConfig::new().set_response_timeout(None);
        let timeout =
            (endpoint.timeout_micros > 0).then(|| Duration::from_micros(endpoint.timeout_micros));
        let mut retries = 0;
        loop {
            match endpoint
                .open(RedisConnectionInfo::default(), config.clone(), timeout)
                .await
            {
                Ok(socket) => return Ok(socket),
                // Only the handshake is retried: no PHP command was sent yet.
                Err(error) if error.is_io_error() && retries < HANDSHAKE_RETRIES => {
                    retries += 1;
                    tokio::time::sleep(HANDSHAKE_PAUSE).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// After an I/O error the socket may be half way through a reply, so it
    /// must not be lent again.
    pub fn retire_on_io_error<T>(&mut self, result: RedisResult<T>) -> RedisResult<T> {
        if result.as_ref().is_err_and(RedisError::is_io_error) {
            self.reusable = false;
        }
        result
    }
}

impl phera_io_pool::Connection for Socket {
    type Error = RedisError;

    /// A PING before an idle socket is lent again, however briefly it idled.
    async fn check(&mut self, _idle: Duration) -> bool {
        matches!(
            redis::cmd("PING")
                .query_async::<Value>(&mut self.connection)
                .await,
            Ok(Value::SimpleString(_))
        )
    }

    fn reusable(&self) -> bool {
        self.reusable
    }

    /// RESET leaves MULTI, ends WATCH and subscriptions, logs out, selects
    /// database 0 and clears the client name.
    async fn reset(&mut self) -> RedisResult<()> {
        redis::cmd("RESET")
            .query_async::<Value>(&mut self.connection)
            .await?;
        Ok(())
    }

    async fn close(self) {}
}
