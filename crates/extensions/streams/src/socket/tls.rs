//! `stream_socket_enable_crypto($socket, true, $method)` negotiates or resumes TLS.

mod material;
mod options;
mod verify;

use std::{
    future::Future,
    io::{self, Read},
    pin::Pin,
    sync::{Arc, atomic::Ordering},
    task::{Context, Poll, Waker},
};

use rustls::{ClientConfig, ServerConfig, pki_types::ServerName};
use tokio::io::AsyncWriteExt;
use tokio_rustls::{TlsAcceptor, TlsConnector, TlsStream};

use super::{Connection, Io, wire::Wire};

pub(super) use material::Material;
pub(super) use options::Options;

type Handshake = Pin<Box<dyn Future<Output = io::Result<TlsStream<Wire>>> + Send>>;

#[derive(Default)]
pub(super) enum State {
    #[default]
    Plain,
    Negotiating(Handshake),
    Established(Box<TlsStream<Wire>>),
    Failed,
}

pub(super) async fn enable(io: &Io, options: &Options, method: i64) -> io::Result<()> {
    start(io, options, method, None).await
}

pub(super) async fn start(
    io: &Io,
    options: &Options,
    method: i64,
    material: Option<Material>,
) -> io::Result<()> {
    let mut state = io.socket.secure.lock().await;
    match &mut *state {
        State::Plain => {
            let material = match material {
                Some(material) => material,
                None => Material::load(options).await?,
            };
            let handshake = handshake(&io.socket, options, method, material)?;
            *state = State::Negotiating(handshake);
        }
        State::Negotiating(_) => (),
        State::Established(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "SSL/TLS already set-up for this stream",
            ));
        }
        State::Failed => return Err(io::ErrorKind::NotConnected.into()),
    }

    let State::Negotiating(handshake) = &mut *state else {
        return Err(io::ErrorKind::NotConnected.into());
    };
    let result = if io.blocked {
        io.wait(handshake).await
    } else {
        let mut cx = Context::from_waker(Waker::noop());
        match handshake.as_mut().poll(&mut cx) {
            Poll::Ready(result) => result,
            Poll::Pending => return Err(io::ErrorKind::WouldBlock.into()),
        }
    };
    match result {
        Ok(stream) => {
            *state = State::Established(Box::new(stream));
            Ok(())
        }
        Err(error) => {
            *state = State::Failed;
            let _ = io.socket.sockref().shutdown(std::net::Shutdown::Both);
            Err(error)
        }
    }
}

fn handshake(
    socket: &Connection,
    options: &Options,
    method: i64,
    material: Material,
) -> io::Result<Handshake> {
    let mut versions = Vec::new();
    if method & 64 != 0 {
        versions.push(&rustls::version::TLS13);
    }
    if method & 32 != 0 {
        versions.push(&rustls::version::TLS12);
    }
    if versions.is_empty() {
        return Err(invalid("No supported SSL/TLS protocol"));
    }
    let provider = phera_io_pool::tls::provider().clone();
    let wire = socket.wire.clone();
    if method & 1 != 0 {
        let name = options
            .peer_name
            .clone()
            .or_else(|| socket.peer.get().cloned())
            .unwrap_or_default();
        let name = ServerName::try_from(name).map_err(invalid)?;
        let verifier = verify::Verifier::new(
            options,
            material.roots,
            provider.signature_verification_algorithms,
        )?;
        let builder = ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(&versions)
            .map_err(invalid)?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier));
        let mut config = if let Some((cert, key)) = material.identity {
            builder.with_client_auth_cert(cert, key).map_err(invalid)?
        } else {
            builder.with_no_client_auth()
        };
        config.enable_sni = options.sni;
        let connector = TlsConnector::from(Arc::new(config));
        Ok(Box::pin(async move {
            connector.connect(name, wire).await.map(TlsStream::from)
        }))
    } else {
        let Some((cert, key)) = material.identity else {
            return Err(invalid("TLS server sockets need a server certificate"));
        };
        let builder = ServerConfig::builder_with_provider(provider.clone())
            .with_protocol_versions(&versions)
            .map_err(invalid)?;
        let builder = if options.verify_peer {
            let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
                Arc::new(material.roots),
                provider,
            )
            .build()
            .map_err(invalid)?;
            builder.with_client_cert_verifier(verifier)
        } else {
            builder.with_no_client_auth()
        };
        let config = builder.with_single_cert(cert, key).map_err(invalid)?;
        let acceptor = TlsAcceptor::from(Arc::new(config));
        Ok(Box::pin(async move {
            acceptor.accept(wire).await.map(TlsStream::from)
        }))
    }
}

pub(super) async fn disable(io: &Io) {
    let mut state = io.socket.secure.lock().await;
    match std::mem::take(&mut *state) {
        State::Established(mut stream) => {
            stream.get_mut().1.send_close_notify();
            let _ = io.wait(stream.flush()).await;
            io.socket.retired.store(true, Ordering::Relaxed);
        }
        State::Negotiating(_) | State::Failed => {
            *state = State::Failed;
            let _ = io.socket.sockref().shutdown(std::net::Shutdown::Both);
        }
        State::Plain => (),
    }
}

pub(super) async fn take_channel(socket: &Connection) -> Option<Box<TlsStream<Wire>>> {
    let mut secure = socket.secure.lock().await;
    if let State::Established(stream) = std::mem::take(&mut *secure) {
        Some(stream)
    } else {
        None
    }
}

pub(super) async fn no_buffered_input(socket: &Connection) -> bool {
    let mut secure = socket.secure.lock().await;
    let mut bytes = [0; 1];
    let result = match &mut *secure {
        State::Plain => return true,
        State::Established(stream) => match stream.as_mut() {
            TlsStream::Client(stream) => stream.get_mut().1.reader().read(&mut bytes),
            TlsStream::Server(stream) => stream.get_mut().1.reader().read(&mut bytes),
        },
        State::Failed | State::Negotiating(_) => return false,
    };
    matches!(result, Err(error) if error.kind() == io::ErrorKind::WouldBlock)
}

pub(super) fn invalid(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, error.to_string())
}
