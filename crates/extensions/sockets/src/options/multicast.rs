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

pub(super) fn interface_index(value: &Value) -> io::Result<u32> {
    match &*value.deref() {
        Value::Int(index) => u32::try_from(*index).map_err(|_| invalid_interface()),
        Value::String(name) => if_nametoindex(name.as_bytes()).map_err(Into::into),
        _ => Err(invalid_interface()),
    }
}

pub(super) fn ipv4_address(index: u32) -> io::Result<Ipv4Addr> {
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

pub(super) fn ipv4_index(socket: &SockRef<'_>) -> io::Result<u32> {
    let address = socket.multicast_if_v4()?;
    if address.is_unspecified() {
        return Ok(0);
    }
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
}

pub(super) fn membership(
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
    let Value::String(group) = &*group.deref() else {
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
    let index = fields
        .get(&ArrayKey::str("interface"))
        .map_or(Ok(0), interface_index);
    Ok(index.and_then(|index| match (address, join) {
        (IpAddr::V4(address), true) => socket.join_multicast_v4(&address, &ipv4_address(index)?),
        (IpAddr::V4(address), false) => socket.leave_multicast_v4(&address, &ipv4_address(index)?),
        (IpAddr::V6(address), true) => socket.join_multicast_v6(&address, index),
        (IpAddr::V6(address), false) => socket.leave_multicast_v6(&address, index),
    }))
}

fn invalid_interface() -> io::Error {
    io::Error::from_raw_os_error(libc::ENODEV)
}
