//! Redis RESET clears MULTI, WATCH, selected database, client name and subscriptions.
use redis::aio::MultiplexedConnection;
use redis::{Client, ConnectionAddr, IntoConnectionInfo, RedisResult, Value};

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
    pub reusable: bool,
}

impl Socket {
    pub async fn connect(endpoint: Endpoint) -> RedisResult<Self> {
        let config = redis::AsyncConnectionConfig::new()
            .set_response_timeout(None)
            .set_connection_timeout(
                (endpoint.timeout_micros > 0)
                    .then(|| std::time::Duration::from_micros(endpoint.timeout_micros)),
            );
        let connection = endpoint
            .client()?
            .get_multiplexed_async_connection_with_config(&config)
            .await?;
        Ok(Self {
            connection,
            reusable: true,
        })
    }
}

impl phera_io_pool::Connection for Socket {
    type Error = redis::RedisError;
    fn reusable(&self) -> bool {
        self.reusable
    }
    async fn reset(&mut self) -> RedisResult<()> {
        redis::cmd("RESET")
            .query_async::<Value>(&mut self.connection)
            .await?;
        Ok(())
    }
    async fn close(self) {
        drop(self);
    }
}
