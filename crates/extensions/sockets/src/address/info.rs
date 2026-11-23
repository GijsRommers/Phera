//! `socket_addrinfo_explain($address)` exposes the endpoint and socket parameters.

use std::{any::Any, io, net::SocketAddr, sync::Arc};

use nix::libc;
use phera_ext_streams::StreamSocket;
use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult,
    Object, Param, ParamType, PhpArray, Registry, Value, args,
    value::{Fork, is_numeric, to_int},
};
use socket2::{Domain, Protocol, Socket, Type};

use crate::{classes, failure};

#[derive(Clone)]
struct AddressInfo {
    endpoint: SocketAddr,
    kind: i32,
    protocol: i32,
    canonical: Option<String>,
}

pub(crate) fn register(registry: &mut Registry) {
    const ADDRESS: &[Param] = &[Param::new("address", ParamType::Class("AddressInfo"))];
    for entry in [
        FunctionEntry::asynchronous(
            "socket_addrinfo_lookup",
            const {
                &[
                    Param::new("host", ParamType::String),
                    Param::new("service", ParamType::String)
                        .nullable()
                        .optional(),
                    Param::new("hints", ParamType::Array).with_default(ConstantValue::EmptyArray),
                ]
            },
            lookup,
        ),
        FunctionEntry::asynchronous("socket_addrinfo_connect", ADDRESS, connect),
        FunctionEntry::asynchronous("socket_addrinfo_bind", ADDRESS, bind),
        FunctionEntry::sync("socket_addrinfo_explain", ADDRESS, explain),
    ] {
        registry.function(entry);
    }
}

pub(crate) fn fork(_: &Object, state: &dyn Any, _: &mut Fork<'_>) -> Result<Box<dyn Any>, String> {
    let address = state
        .downcast_ref::<AddressInfo>()
        .ok_or("unknown AddressInfo state")?;
    Ok(Box::new(address.clone()))
}

fn info(args: &[Value]) -> Result<AddressInfo, NativeError> {
    args::object(args, 0)
        .and_then(|object| {
            object
                .native
                .borrow()
                .as_ref()?
                .downcast_ref::<AddressInfo>()
                .cloned()
        })
        .ok_or_else(|| NativeError::error("AddressInfo is not initialized"))
}

fn explain(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let info = info(args)?;
    let mut address = PhpArray::new();
    let (family, port_name, ip_name) = match info.endpoint {
        SocketAddr::V4(_) => (libc::AF_INET, "sin_port", "sin_addr"),
        SocketAddr::V6(_) => (libc::AF_INET6, "sin6_port", "sin6_addr"),
    };
    address.insert(
        ArrayKey::str(port_name),
        Value::Int(i64::from(info.endpoint.port())),
    );
    address.insert(
        ArrayKey::str(ip_name),
        Value::from(info.endpoint.ip().to_string()),
    );
    let mut data = PhpArray::new();
    for (name, number) in [
        ("ai_flags", 0),
        ("ai_family", family),
        ("ai_socktype", info.kind),
        ("ai_protocol", info.protocol),
    ] {
        data.insert(ArrayKey::str(name), Value::Int(i64::from(number)));
    }
    if let Some(name) = info.canonical {
        data.insert(ArrayKey::str("ai_canonname"), Value::from(name));
    }
    data.insert(ArrayKey::str("ai_addr"), Value::from(address));
    Ok(Value::from(data))
}

#[derive(Default)]
struct Hints {
    family: i32,
    kind: i32,
    protocol: i32,
    flags: i32,
}

impl Hints {
    fn from_php(context: &mut dyn Context, hints: Option<&PhpArray>) -> Result<Self, NativeError> {
        const ERROR: &str = "socket_addrinfo_lookup(): Argument #3 ($hints)";
        let mut parsed = Self::default();
        if let Some(hints) = hints {
            for (key, value) in hints.iter() {
                let name = key.as_str_bytes().unwrap_or(b"");
                let target = match name {
                    b"ai_family" => &mut parsed.family,
                    b"ai_socktype" => &mut parsed.kind,
                    b"ai_protocol" => &mut parsed.protocol,
                    b"ai_flags" => &mut parsed.flags,
                    _ => {
                        return Err(NativeError::value_error(format!(
                            "{ERROR} must only contain array keys \"ai_flags\", \"ai_socktype\", \"ai_protocol\", or \"ai_family\""
                        )));
                    }
                };
                match &*value.deref() {
                    Value::Int(_) | Value::Float(_) | Value::Bool(_) | Value::Null => {}
                    Value::InternedStr(value) if is_numeric(value.as_bytes()) => {}
                    Value::String(value) if is_numeric(value.as_bytes()) => {}
                    _ => {
                        return Err(NativeError::type_error(format!(
                            "{ERROR} \"{}\" key must be of type int, {} given",
                            String::from_utf8_lossy(name),
                            value.type_name()
                        )));
                    }
                }
                let number = to_int(value, context.diagnostics())?;
                if name == b"ai_family"
                    && number != i64::from(libc::AF_INET)
                    && number != i64::from(libc::AF_INET6)
                {
                    return Err(NativeError::value_error(format!(
                        "{ERROR} \"ai_family\" key must be AF_INET or AF_INET6"
                    )));
                }
                *target = i32::try_from(number)
                    .ok()
                    .filter(|value| *value >= 0)
                    .ok_or_else(|| {
                        NativeError::value_error(format!(
                            "{ERROR} \"{}\" key must be between 0 and 2147483647",
                            String::from_utf8_lossy(name)
                        ))
                    })?;
            }
        }
        Ok(parsed)
    }

    fn types(&self, numeric_service: bool) -> Vec<(i32, i32)> {
        let tcp = (libc::SOCK_STREAM, libc::IPPROTO_TCP);
        let udp = (libc::SOCK_DGRAM, libc::IPPROTO_UDP);
        let raw = (libc::SOCK_RAW, self.protocol);
        let mut kinds = if cfg!(target_os = "linux") {
            vec![tcp, udp]
        } else {
            vec![udp, tcp]
        };
        if self.kind == libc::SOCK_RAW
            || self.kind == 0 && numeric_service && cfg!(target_os = "linux")
        {
            kinds.push(raw);
        }
        kinds.retain(|&(kind, protocol)| {
            (self.kind == 0 || self.kind == kind)
                && (self.protocol == 0 || self.protocol == protocol)
        });
        kinds
    }

    fn accepts(&self, address: &SocketAddr) -> bool {
        match self.family {
            libc::AF_INET => address.is_ipv4(),
            libc::AF_INET6 => address.is_ipv6(),
            _ => true,
        }
    }
}

fn lookup(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let host = args::string(&args, 0)
            .map(|value| value.to_string_lossy())
            .unwrap_or_default();
        let service = args::string(&args, 1)
            .map(|value| value.to_string_lossy())
            .unwrap_or_default();
        let hints = Hints::from_php(context, args::array(&args, 2).map(std::rc::Rc::as_ref))?;
        if hints.flags & libc::AI_NUMERICHOST != 0 && host.parse::<std::net::IpAddr>().is_err() {
            return Ok(Value::Bool(false));
        }
        let numeric = if service.is_empty() {
            Some(0)
        } else {
            service.parse::<u16>().ok()
        };
        if numeric.is_none() && hints.flags & libc::AI_NUMERICSERV != 0 {
            return Ok(Value::Bool(false));
        }
        let types = hints.types(numeric.is_some());
        if types.is_empty() {
            return Ok(Value::Bool(false));
        }
        let Ok(addresses) = tokio::net::lookup_host((host.as_ref(), 0)).await else {
            return Ok(Value::Bool(false));
        };
        let addresses: Vec<_> = addresses.filter(|address| hints.accepts(address)).collect();
        let mut ports = Vec::new();
        for (kind, protocol) in types {
            let port = match numeric {
                Some(port) => Some(port),
                None => service_port(&service, protocol).await,
            };
            if let Some(port) = port {
                ports.push((kind, protocol, port));
            }
        }
        let mut result = PhpArray::new();
        for mut endpoint in addresses {
            for &(kind, protocol, port) in &ports {
                endpoint.set_port(port);
                let canonical = (result.is_empty() && hints.flags & libc::AI_CANONNAME != 0)
                    .then(|| host.to_string());
                result.push(classes::create(
                    context,
                    "AddressInfo",
                    AddressInfo {
                        endpoint,
                        kind,
                        protocol,
                        canonical,
                    },
                )?)?;
            }
        }
        Ok(if result.is_empty() {
            Value::Bool(false)
        } else {
            Value::from(result)
        })
    })
}

async fn service_port(service: &str, protocol: i32) -> Option<u16> {
    let contents = tokio::fs::read_to_string("/etc/services").await.ok()?;
    let protocol = if protocol == libc::IPPROTO_TCP {
        "tcp"
    } else {
        "udp"
    };
    for line in contents.lines() {
        let mut fields = line.split('#').next()?.split_ascii_whitespace();
        let (Some(name), Some((port, transport))) = (
            fields.next(),
            fields.next().and_then(|field| field.split_once('/')),
        ) else {
            continue;
        };
        if transport == protocol && (name == service || fields.any(|alias| alias == service)) {
            return port.parse().ok();
        }
    }
    None
}

fn connect(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(open(context, args, true))
}

fn bind(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(open(context, args, false))
}

async fn open(context: &mut dyn Context, args: Vec<Value>, connecting: bool) -> NativeResult {
    let function = if connecting {
        "socket_addrinfo_connect"
    } else {
        "socket_addrinfo_bind"
    };
    let info = info(&args)?;
    match open_address(info, connecting).await {
        Ok(socket) => classes::socket(context, socket, Value::Null),
        Err(error) => failure(context, None, function, "Unable to create socket", &error),
    }
}

async fn open_address(info: AddressInfo, connecting: bool) -> io::Result<Arc<StreamSocket>> {
    let endpoint = info.endpoint.into();
    let domain = Domain::for_address(info.endpoint);
    let kind = Type::from(info.kind);
    let protocol = (info.protocol != 0).then(|| Protocol::from(info.protocol));
    let descriptor = Socket::new(domain, kind, protocol)?;
    if !connecting {
        descriptor.bind(&endpoint)?;
    }
    let socket = Arc::new(StreamSocket::register_descriptor(
        descriptor, domain, kind, false,
    )?);
    if connecting {
        socket
            .connect()
            .await?
            .connect_descriptor(&endpoint, true)
            .await?;
    }
    Ok(socket)
}
