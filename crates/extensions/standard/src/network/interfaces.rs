//! `net_get_interfaces()` describes the operating system's network interfaces.

use indexmap::IndexMap;
use nix::{
    ifaddrs::getifaddrs,
    net::if_::InterfaceFlags,
    sys::socket::{SockaddrLike, SockaddrStorage},
};
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeResult, PhpArray, Registry, Value,
};

pub(super) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::asynchronous("net_get_interfaces", &[], list));
}

struct Interface {
    up: bool,
    addresses: Vec<Address>,
}

struct Address {
    flags: i64,
    family: i64,
    address: Option<String>,
    netmask: Option<String>,
    broadcast: Option<String>,
    point_to_point: Option<String>,
}

fn format_address(address: &SockaddrStorage) -> Option<String> {
    if let Some(address) = address.as_sockaddr_in() {
        return Some(address.ip().to_string());
    }
    let address = address.as_sockaddr_in6()?;
    let mut ip = address.ip();
    if cfg!(target_vendor = "apple") && ip.is_unicast_link_local() {
        let mut bytes = ip.octets();
        bytes[2..4].fill(0);
        ip = bytes.into();
    }
    Some(ip.to_string())
}

fn enumerate() -> nix::Result<IndexMap<String, Interface>> {
    let mut interfaces = IndexMap::new();
    for item in getifaddrs()? {
        let interface = interfaces
            .entry(item.interface_name)
            .or_insert_with(|| Interface {
                up: item.flags.contains(InterfaceFlags::IFF_UP),
                addresses: Vec::new(),
            });
        let Some(address) = item.address else {
            continue;
        };
        let Some(family) = address.family() else {
            continue;
        };
        interface.addresses.push(Address {
            flags: i64::from(item.flags.bits()),
            family: family as i64,
            address: format_address(&address),
            netmask: item.netmask.as_ref().and_then(format_address),
            broadcast: item.broadcast.as_ref().and_then(format_address),
            point_to_point: item.destination.as_ref().and_then(format_address),
        });
    }
    Ok(interfaces)
}

fn list(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let interfaces = match tokio::task::spawn_blocking(enumerate).await {
            Ok(Ok(interfaces)) => interfaces,
            Ok(Err(error)) => {
                context.warning(format!("net_get_interfaces(): {error}"));
                return Ok(Value::Bool(false));
            }
            Err(_) => return Ok(Value::Bool(false)),
        };
        let mut output = PhpArray::new();
        for (name, interface) in interfaces {
            let mut unicast = PhpArray::new();
            for address in interface.addresses {
                let mut entry = PhpArray::new();
                entry.insert(ArrayKey::str("flags"), Value::Int(address.flags));
                entry.insert(ArrayKey::str("family"), Value::Int(address.family));
                for (key, value) in [
                    ("address", address.address),
                    ("netmask", address.netmask),
                    ("broadcast", address.broadcast),
                    ("ptp", address.point_to_point),
                ] {
                    if let Some(value) = value {
                        entry.insert(ArrayKey::str(key), Value::from(value));
                    }
                }
                unicast.push(Value::from(entry))?;
            }
            let mut entry = PhpArray::new();
            entry.insert(ArrayKey::str("unicast"), Value::from(unicast));
            entry.insert(ArrayKey::str("up"), Value::Bool(interface.up));
            output.insert(ArrayKey::str(name), Value::from(entry));
        }
        Ok(Value::from(output))
    })
}
