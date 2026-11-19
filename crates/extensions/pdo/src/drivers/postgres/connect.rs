//! `sslmode=prefer` asks for TLS before sending PostgreSQL startup data.
//! A server that answers `N` can still accept the same socket in plaintext.

use std::io;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::time::Duration;

use tokio::net::{TcpStream, UnixStream};
use tokio_postgres::{Client, Config, Connection, NoTls, tls::NoTlsStream};

use super::stream::ObservedStream;
use super::tls::{Mode, negotiate};
use crate::drivers::connection_error;
use crate::dsn::Dsn;
use crate::errors::DbError;

type Connected = (Client, Connection<ObservedStream, NoTlsStream>);

pub(super) async fn open(
    dsn: &Dsn,
    user: &str,
    password: &str,
    status: Arc<AtomicU8>,
) -> Result<Connected, DbError> {
    let mode = Mode::parse(dsn.get("sslmode", "prefer"))?;
    let host = dsn.get("host", "localhost");
    let port = dsn.port()?;
    let mut config = Config::new();
    config
        .dbname(dsn.get("dbname", user))
        .user(user)
        .password(password)
        .ssl_mode(tokio_postgres::config::SslMode::Disable);
    if let Some(options) = dsn.fields.get("options") {
        config.options(options);
    }
    if let Some(name) = dsn.fields.get("application_name") {
        config.application_name(name);
    }
    let connect = async {
        if host.starts_with('/') {
            let path = format!("{host}/.s.PGSQL.{port}");
            let socket = UnixStream::connect(&path).await.map_err(connection_error)?;
            return startup(
                &config,
                ObservedStream::new(Box::new(socket), status),
                host,
                port,
            )
            .await
            .map_err(|(error, _)| error);
        }
        let address = dsn.get("hostaddr", host);
        let socket = tcp(address, port).await?;
        let socket = negotiate(socket, dsn, host, port, mode).await?;
        match startup(
            &config,
            ObservedStream::new(socket, status.clone()),
            host,
            port,
        )
        .await
        {
            Err((_, true)) if mode == Mode::Allow => {
                let socket = tcp(address, port).await?;
                let socket = negotiate(socket, dsn, host, port, Mode::Require).await?;
                startup(&config, ObservedStream::new(socket, status), host, port)
                    .await
                    .map_err(|(error, _)| error)
            }
            result => result.map_err(|(error, _)| error),
        }
    };
    let duration = u64::try_from(dsn.options.timeout.unwrap_or(30))
        .ok()
        .filter(|n| *n > 0);
    match duration {
        Some(seconds) => tokio::time::timeout(Duration::from_secs(seconds.max(2)), connect)
            .await
            .map_err(|_| failed(host, port, "timeout expired"))?,
        None => connect.await,
    }
}

async fn tcp(address: &str, port: u16) -> Result<TcpStream, DbError> {
    let socket = TcpStream::connect((address, port))
        .await
        .map_err(connection_error)?;
    socket.set_nodelay(true).map_err(connection_error)?;
    Ok(socket)
}

async fn startup(
    config: &Config,
    stream: ObservedStream,
    host: &str,
    port: u16,
) -> Result<Connected, (DbError, bool)> {
    config.connect_raw(stream, NoTls).await.map_err(|error| {
        let detail = match error.as_db_error() {
            Some(database) => format!("{}:  {}", database.severity(), database.message()),
            None => error.to_string(),
        };
        (failed(host, port, detail), error.as_db_error().is_some())
    })
}

pub(super) fn failed(host: &str, port: u16, detail: impl std::fmt::Display) -> DbError {
    connection_error(format!(
        "connection to server at \"{host}\", port {port} failed: {detail}"
    ))
}

pub(super) fn tls_failure(host: &str, port: u16, error: io::Error) -> DbError {
    use rustls::{CertificateError, Error};

    let problem = error
        .get_ref()
        .and_then(|source| source.downcast_ref::<Error>());
    let reason = match problem {
        Some(Error::InvalidCertificate(
            CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. },
        )) => {
            return failed(
                host,
                port,
                format!("server certificate does not match host name \"{host}\""),
            );
        }
        Some(Error::InvalidCertificate(_)) => "SSL error: certificate verify failed".to_owned(),
        _ => format!("SSL error: {error}"),
    };
    failed(host, port, reason)
}
