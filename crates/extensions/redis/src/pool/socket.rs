//! The pooled socket: where it connects, and what the pool may do with it.
use redis::aio::MultiplexedConnection;
use std::time::Duration;

use redis::{Client, ConnectionAddr, IntoConnectionInfo, RedisError, RedisResult, Value};

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub timeout_micros: u64,
    pub persistent_id: Option<String>,
}

impl Endpoint {
    pub fn client(&self) -> RedisResult<Client> {
        static CRYPTO: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        CRYPTO.get_or_init(|| {
            let _ = rustls_rustcrypto::provider().install_default();
        });
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
        } else if let Some(host) = self.host.strip_prefix("tls://") {
            ConnectionAddr::TcpTls {
                host: host.to_owned(),
                port: self.port,
                insecure: false,
                tls_params: None,
            }
        } else {
            ConnectionAddr::Tcp(self.host.trim_start_matches("tcp://").to_owned(), self.port)
        };
        Client::open(("127.0.0.1", 6379).into_connection_info()?.set_addr(addr))
    }
}

pub struct Socket {
    pub connection: MultiplexedConnection,
    /// Cleared after an I/O error, or by `Lease::retire`; the pool then
    /// closes the socket instead of lending it again.
    pub reusable: bool,
}

/// A failed handshake is tried this many more times, after a short pause.
const HANDSHAKE_RETRIES: u32 = 2;
const HANDSHAKE_PAUSE: Duration = Duration::from_millis(2);

impl Socket {
    pub async fn connect(endpoint: Endpoint) -> RedisResult<Self> {
        let config = redis::AsyncConnectionConfig::new()
            .set_response_timeout(None)
            .set_connection_timeout(
                (endpoint.timeout_micros > 0)
                    .then(|| Duration::from_micros(endpoint.timeout_micros)),
            );
        let client = endpoint.client()?;
        let mut retries = 0;
        loop {
            match client
                .get_multiplexed_async_connection_with_config(&config)
                .await
            {
                Ok(connection) => {
                    return Ok(Self {
                        connection,
                        reusable: true,
                    });
                }
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
