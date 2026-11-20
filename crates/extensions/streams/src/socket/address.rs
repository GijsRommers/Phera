//! `stream_socket_client('tcp://[::1]:8080')` separates the transport and address.

use std::io;
use std::net::SocketAddr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use socket2::{Domain, SockAddr, Type};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Transport {
    Tcp,
    Udp,
    Unix,
    UnixDatagram,
    Tls(i64),
}

pub(super) const NAMES: &[&str] = &[
    "tcp", "udp", "unix", "udg", "ssl", "sslv3", "tls", "tlsv1.0", "tlsv1.1", "tlsv1.2", "tlsv1.3",
];

impl Transport {
    pub(super) fn parse(address: &[u8]) -> Result<(Self, &[u8]), String> {
        let Some(index) = address.windows(3).position(|part| part == b"://") else {
            return Ok((Self::Tcp, address));
        };
        let scheme = &address[..index];
        let kind = match scheme {
            b"tcp" => Self::Tcp,
            b"udp" => Self::Udp,
            b"unix" => Self::Unix,
            b"udg" => Self::UnixDatagram,
            b"ssl" | b"tls" => Self::Tls(121),
            b"sslv3" => Self::Tls(5),
            b"tlsv1.0" => Self::Tls(9),
            b"tlsv1.1" => Self::Tls(17),
            b"tlsv1.2" => Self::Tls(33),
            b"tlsv1.3" => Self::Tls(65),
            _ => {
                return Err(format!(
                    "Unable to find the socket transport \"{}\" - did you forget to enable it when you configured PHP?",
                    String::from_utf8_lossy(scheme)
                ));
            }
        };
        Ok((kind, &address[index + 3..]))
    }

    pub(super) fn datagram(self) -> bool {
        matches!(self, Self::Udp | Self::UnixDatagram)
    }

    pub(super) fn unix(self) -> bool {
        matches!(self, Self::Unix | Self::UnixDatagram)
    }

    pub(super) fn socket_type(self) -> Type {
        if self.datagram() {
            Type::DGRAM
        } else {
            Type::STREAM
        }
    }

    pub(super) fn stream_type(self) -> &'static str {
        match self {
            Self::Tcp => "tcp_socket/ssl",
            Self::Tls(_) => "tcp_socket/ssl",
            Self::Udp => "udp_socket",
            Self::Unix | Self::UnixDatagram => "unix_socket",
        }
    }
}

pub(super) struct Address<'a> {
    pub transport: Transport,
    pub host: &'a str,
    port: u16,
    unix: Option<SockAddr>,
}

impl<'a> Address<'a> {
    pub(super) fn parse(transport: Transport, name: &'a [u8]) -> Result<Self, String> {
        let malformed = || {
            format!(
                "Failed to parse address \"{}\"",
                String::from_utf8_lossy(name)
            )
        };
        if transport.unix() {
            let path = Path::new(std::ffi::OsStr::from_bytes(name));
            let unix = SockAddr::unix(path).map_err(|error| error.to_string())?;
            return Ok(Self {
                transport,
                host: "",
                port: 0,
                unix: Some(unix),
            });
        }
        let name = std::str::from_utf8(name).map_err(|_| malformed())?;
        let (host, port) = name.rsplit_once(':').ok_or_else(malformed)?;
        let host = host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(host);
        let port = port.parse().map_err(|_| malformed())?;
        Ok(Self {
            transport,
            host,
            port,
            unix: None,
        })
    }

    pub(super) async fn resolve(&self) -> io::Result<Vec<SockAddr>> {
        if let Some(unix) = &self.unix {
            return Ok(vec![unix.clone()]);
        }
        let host = if self.host.is_empty() {
            "0.0.0.0"
        } else {
            self.host
        };
        if let Ok(address) = host.parse::<std::net::IpAddr>() {
            return Ok(vec![SocketAddr::new(address, self.port).into()]);
        }
        Ok(tokio::net::lookup_host((host, self.port))
            .await?
            .map(SockAddr::from)
            .collect())
    }
}

pub(super) fn name(address: &SockAddr) -> Option<Vec<u8>> {
    if let Some(address) = address.as_socket() {
        return Some(address.to_string().into_bytes());
    }
    if address.domain() == Domain::UNIX {
        return address
            .as_pathname()
            .map(|path| path.as_os_str().as_bytes().to_vec());
    }
    None
}
