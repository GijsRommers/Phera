//! `sslmode=prefer` asks for TLS before sending PostgreSQL startup data.
//! A server that answers `N` can still accept the same socket in plaintext.

use std::io;
use std::time::Duration;

use tokio::net::{TcpStream, UnixStream};

use super::stream::Socket;
use super::tls::{Mode, negotiate};
use super::wire::{Client, Transport, startup};
use super::{Postgres, native};
use crate::drivers::connection_error;
use crate::dsn::Dsn;
use crate::errors::DbError;

async fn open(dsn: &Dsn, user: &str, password: &str) -> Result<Client, DbError> {
    let system_roots = dsn.get("sslrootcert", "") == "system";
    let default_mode = if system_roots {
        "verify-full"
    } else {
        "prefer"
    };
    let mode_text = dsn.get("sslmode", default_mode);
    let mode = Mode::parse(mode_text)?;
    if system_roots && mode != Mode::VerifyFull {
        return Err(connection_error(format!(
            "weak sslmode \"{mode_text}\" may not be used with sslrootcert=system \
             (use \"verify-full\")"
        )));
    }
    let host = dsn.get("host", "localhost");
    let port = dsn.port()?;
    let mut parameters = vec![
        ("database", dsn.get("dbname", user)),
        ("user", user),
        ("client_encoding", "UTF8"),
    ];
    for key in ["options", "application_name"] {
        if let Some(value) = dsn.fields.get(key) {
            parameters.push((key, value));
        }
    }
    let connect = async {
        if host.starts_with('/') {
            if matches!(mode, Mode::Require | Mode::VerifyCa | Mode::VerifyFull) {
                return Err(failed(
                    host,
                    port,
                    "SSL is unavailable over a Unix-domain socket",
                ));
            }
            let path = format!("{host}/.s.PGSQL.{port}");
            let socket = UnixStream::connect(&path).await.map_err(connection_error)?;
            return start(Box::new(socket), &parameters, user, password, host, port)
                .await
                .map_err(|(error, _)| error);
        }
        let address = dsn.get("hostaddr", host);
        let socket = negotiate(tcp(address, port).await?, dsn, host, port, mode).await?;
        match start(socket, &parameters, user, password, host, port).await {
            Err((_, true)) if mode == Mode::Allow => {
                let socket =
                    negotiate(tcp(address, port).await?, dsn, host, port, Mode::Require).await?;
                start(socket, &parameters, user, password, host, port)
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

async fn start(
    socket: Box<dyn Socket>,
    parameters: &[(&str, &str)],
    user: &str,
    password: &str,
    host: &str,
    port: u16,
) -> Result<Client, (DbError, bool)> {
    let mut transport = Transport::new(socket);
    match startup::start(&mut transport, parameters, user, password.as_bytes()).await {
        Ok(session) => Ok(Client::new(transport, session)),
        Err(startup::StartError::Transport(error)) => Err((failed(host, port, error), false)),
        Err(startup::StartError::Server(error)) => Err((failed(host, port, error.detail), true)),
    }
}

pub(super) fn failed(host: &str, port: u16, detail: impl std::fmt::Display) -> DbError {
    connection_error(format!(
        "connection to server at \"{host}\", port {port} failed: {detail}"
    ))
}

pub(super) fn tls_failure(host: &str, port: u16, error: io::Error) -> DbError {
    use rustls::{AlertDescription, CertificateError, Error};

    let problem = error
        .get_ref()
        .and_then(|source| source.downcast_ref::<Error>());
    let reason = match problem {
        Some(Error::InvalidCertificate(CertificateError::NotValidForNameContext {
            presented,
            ..
        })) => certificate_name_error(presented, host),
        Some(Error::InvalidCertificate(CertificateError::NotValidForName)) => {
            certificate_name_error(&[], host)
        }
        Some(Error::InvalidCertificate(_)) => "SSL error: certificate verify failed".to_owned(),
        Some(Error::AlertReceived(AlertDescription::ProtocolVersion)) => {
            "SSL error: tlsv1 alert protocol version".to_owned()
        }
        _ => format!("SSL error: {error}"),
    };
    failed(host, port, reason)
}

fn certificate_name_error(presented: &[String], host: &str) -> String {
    let Some((first, rest)) = presented.split_first() else {
        return "could not get server's host name from server certificate".into();
    };
    let extra = match rest.len() {
        0 => String::new(),
        1 => " (and 1 other name)".into(),
        count => format!(" (and {count} other names)"),
    };
    format!("server certificate for \"{first}\"{extra} does not match host name \"{host}\"")
}

impl Postgres {
    pub async fn reconnect(
        &mut self,
        dsn: &Dsn,
        user: &str,
        password: &str,
    ) -> Result<(), DbError> {
        match Self::connect(dsn, user, password).await {
            Ok(connection) => {
                *self = connection;
                Ok(())
            }
            Err(error) => {
                self.client.invalid("PostgreSQL reconnect failed");
                Err(error)
            }
        }
    }

    pub async fn connect(dsn: &Dsn, user: &str, password: &str) -> Result<Self, DbError> {
        let client = open(dsn, user, password).await?;
        let version = client
            .parameter("server_version")
            .unwrap_or_default()
            .to_owned();
        Ok(Self {
            client,
            version,
            lazy: None,
            native: dsn.options.postgres_native.then(native::State::default),
            statements: Vec::new(),
        })
    }
}
