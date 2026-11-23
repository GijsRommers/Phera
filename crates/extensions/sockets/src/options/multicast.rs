//! `socket_set_option($socket, IPPROTO_IP, MCAST_JOIN_GROUP, $group)` joins a multicast group.

use std::{
    io,
    net::{IpAddr, Ipv4Addr},
};

use nix::{
    ifaddrs::getifaddrs,
    libc,
    net::if_::{if_indextoname, if_nametoindex},
};
use phera_extension_api::{ArrayKey, NativeError, Value};
use socket2::{Domain, SockRef};

use super::option_fields;

#[cfg(not(target_vendor = "apple"))]
pub(crate) const MCAST_JOIN_GROUP: i32 = libc::MCAST_JOIN_GROUP;
#[cfg(not(target_vendor = "apple"))]
pub(crate) const MCAST_LEAVE_GROUP: i32 = libc::MCAST_LEAVE_GROUP;
// The Apple SDK defines these options in netinet/in.h. libc does not export them.
#[cfg(target_vendor = "apple")]
pub(crate) const MCAST_JOIN_GROUP: i32 = 80;
#[cfg(target_vendor = "apple")]
pub(crate) const MCAST_LEAVE_GROUP: i32 = 81;

async fn lookup<T: Send + 'static>(
    work: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> io::Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(io::Error::other)?
}

pub(super) enum Interface {
    Index(u32),
    Name(Vec<u8>),
}

impl Interface {
    pub(super) fn from_value(value: &Value) -> io::Result<Self> {
        match &*value.deref() {
            Value::Int(index) => u32::try_from(*index)
                .map(Self::Index)
                .map_err(|_| invalid_interface()),
            Value::InternedStr(name) => Ok(Self::Name(name.as_bytes().to_vec())),
            Value::String(name) => Ok(Self::Name(name.as_bytes().to_vec())),
            _ => Err(invalid_interface()),
        }
    }

    fn resolve(self) -> io::Result<u32> {
        match self {
            Self::Index(index) => Ok(index),
            Self::Name(name) => if_nametoindex(name.as_slice()).map_err(Into::into),
        }
    }

    pub(super) async fn index(self) -> io::Result<u32> {
        match self {
            Self::Index(index) => Ok(index),
            Self::Name(_) => lookup(move || self.resolve()).await,
        }
    }

    pub(super) async fn ipv4(self) -> io::Result<Ipv4Addr> {
        match self {
            Self::Index(0) => Ok(Ipv4Addr::UNSPECIFIED),
            _ => lookup(move || ipv4_address(self.resolve()?)).await,
        }
    }
}

fn ipv4_address(index: u32) -> io::Result<Ipv4Addr> {
    if index == 0 {
        return Ok(Ipv4Addr::UNSPECIFIED);
    }
    let name = if_indextoname(index)?;
    for interface in getifaddrs()? {
        if interface.interface_name.as_bytes() == name.as_bytes()
            && let Some(address) = interface
                .address
                .and_then(|address| address.as_sockaddr_in().map(|address| address.ip()))
        {
            return Ok(address);
        }
    }
    Err(invalid_interface())
}

pub(super) async fn ipv4_index(socket: &SockRef<'_>) -> io::Result<u32> {
    let address = socket.multicast_if_v4()?;
    if address.is_unspecified() {
        return Ok(0);
    }
    lookup(move || {
        for interface in getifaddrs()? {
            if interface
                .address
                .as_ref()
                .and_then(|address| address.as_sockaddr_in())
                .is_some_and(|candidate| candidate.ip() == address)
            {
                return if_nametoindex(interface.interface_name.as_str()).map_err(Into::into);
            }
        }
        Err(invalid_interface())
    })
    .await
}

pub(super) async fn membership(
    socket: &SockRef<'_>,
    option: i32,
    value: &Value,
    function: &str,
) -> Result<io::Result<()>, NativeError> {
    let join = option == MCAST_JOIN_GROUP;
    let name = if join {
        "MCAST_JOIN_GROUP"
    } else {
        "MCAST_LEAVE_GROUP"
    };
    let fields = option_fields(value, function, name)?;
    let key = ArrayKey::str("group");
    let group = fields.get(&key).ok_or_else(|| {
        NativeError::value_error(format!(
            "{function}(): Argument #4 ($value) must have key \"group\""
        ))
    })?;
    let group = group.deref();
    let Some(group) = group.str_ref() else {
        return Err(NativeError::type_error("Multicast group must be a string"));
    };
    let Ok(address) = group.to_string_lossy().parse::<IpAddr>() else {
        return Ok(Err(io::Error::from_raw_os_error(libc::EINVAL)));
    };
    let domain = match socket.local_addr() {
        Ok(address) => address.domain(),
        Err(error) => return Ok(Err(error)),
    };
    if !matches!(
        (domain, address),
        (Domain::IPV4, IpAddr::V4(_)) | (Domain::IPV6, IpAddr::V6(_))
    ) {
        return Err(NativeError::value_error(
            "IP address used in the context of an unexpected type of socket",
        ));
    }
    let interface = match fields
        .get(&ArrayKey::str("interface"))
        .map_or(Ok(Interface::Index(0)), Interface::from_value)
    {
        Ok(interface) => interface,
        Err(error) => return Ok(Err(error)),
    };
    Ok(match address {
        IpAddr::V4(address) => interface.ipv4().await.and_then(|local| {
            if join {
                socket.join_multicast_v4(&address, &local)
            } else {
                socket.leave_multicast_v4(&address, &local)
            }
        }),
        IpAddr::V6(address) => interface.index().await.and_then(|index| {
            if join {
                socket.join_multicast_v6(&address, index)
            } else {
                socket.leave_multicast_v6(&address, index)
            }
        }),
    })
}

fn invalid_interface() -> io::Error {
    io::Error::from_raw_os_error(libc::ENODEV)
}
