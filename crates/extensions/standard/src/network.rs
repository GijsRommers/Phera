//! `inet_pton('::1')` packs an address and `inet_ntop($packed)` formats it.
//! `ip2long('255.255.255.255')` returns 4294967295 on 64-bit PHP.

use std::net::{Ipv4Addr, Ipv6Addr};

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

mod dns;
mod host;
mod interfaces;
mod services;

pub(crate) fn register(registry: &mut Registry) {
    const TEXT: &[Param] = &[Param::new("ip", ParamType::String)];
    const NUMBER: &[Param] = &[Param::new("ip", ParamType::Int)];
    for entry in [
        FunctionEntry::sync("ip2long", TEXT, ip2long),
        FunctionEntry::sync("long2ip", NUMBER, long2ip),
        FunctionEntry::sync("inet_pton", TEXT, pack),
        FunctionEntry::sync("inet_ntop", TEXT, unpack),
    ] {
        registry.function(entry);
    }
    host::register(registry);
    dns::register(registry);
    interfaces::register(registry);
    services::register(registry);
}

fn checked_argument<'a>(
    arguments: &'a [Value],
    index: usize,
    function: &str,
    name: &str,
) -> Result<&'a [u8], NativeError> {
    let bytes = args::bytes(arguments, index);
    if bytes.contains(&0) {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #{} (${name}) must not contain any null bytes",
            index + 1,
        )));
    }
    Ok(bytes)
}

fn checked_text<'a>(arguments: &'a [Value], name: &str) -> Result<Option<&'a str>, NativeError> {
    let input = args::bytes(arguments, 0);
    if input.contains(&0) {
        return Err(NativeError::value_error(format!(
            "{name}(): Argument #1 ($ip) must not contain any null bytes"
        )));
    }
    Ok(std::str::from_utf8(input).ok())
}

fn ipv4(text: &str) -> Option<Ipv4Addr> {
    if !cfg!(target_vendor = "apple") {
        return text.parse().ok();
    }
    // Darwin permits padded decimal octets while Linux rejects them.
    let mut parts = text.split('.');
    let mut bytes = [0; 4];
    for octet in &mut bytes {
        let part = parts.next()?;
        if !part.as_bytes().iter().all(u8::is_ascii_digit) {
            return None;
        }
        *octet = part.parse().ok()?;
    }
    parts.next().is_none().then_some(Ipv4Addr::from(bytes))
}

fn ipv6(text: &str) -> Option<Ipv6Addr> {
    let (text, zone) = if cfg!(target_vendor = "apple") {
        text.split_once('%')
            .map_or((text, None), |(host, zone)| (host, Some(zone)))
    } else {
        (text, None)
    };
    let mut address: Ipv6Addr = if cfg!(target_vendor = "apple") && text.contains('.') {
        let (head, last) = text.rsplit_once(':')?;
        format!("{head}:{}", ipv4(last)?).parse().ok()?
    } else {
        text.parse().ok()?
    };
    if let Some(zone) = zone {
        let scope = nix::net::if_::if_nametoindex(zone)
            .ok()
            .and_then(|index| u16::try_from(index).ok())
            .unwrap_or(0);
        let mut bytes = address.octets();
        if address.is_unicast_link_local() || bytes[0..2] == [0xff, 0x02] {
            bytes[2..4].copy_from_slice(&scope.to_be_bytes());
        }
        address = Ipv6Addr::from(bytes);
    }
    Some(address)
}

fn ip2long(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(checked_text(arguments, "ip2long")?
        .and_then(ipv4)
        .map_or(Value::Bool(false), |ip| {
            Value::Int(i64::from(u32::from(ip)))
        }))
}

fn long2ip(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let ip = Ipv4Addr::from(args::int(arguments, 0).unwrap_or(0) as u32);
    Ok(Value::from(ip.to_string()))
}

fn pack(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let Some(text) = checked_text(arguments, "inet_pton")? else {
        return Ok(Value::Bool(false));
    };
    Ok(if text.contains(':') {
        ipv6(text).map_or(Value::Bool(false), |ip| Value::from(ip.octets().as_slice()))
    } else {
        ipv4(text).map_or(Value::Bool(false), |ip| Value::from(ip.octets().as_slice()))
    })
}

fn unpack(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let bytes = args::bytes(arguments, 0);
    if let Ok(bytes) = <[u8; 4]>::try_from(bytes) {
        return Ok(Value::from(Ipv4Addr::from(bytes).to_string()));
    }
    let Ok(bytes) = <[u8; 16]>::try_from(bytes) else {
        return Ok(Value::Bool(false));
    };
    let address = Ipv6Addr::from(bytes);
    let segments = address.segments();
    let display = if segments[..6] == [0; 6] && segments[6] != 0 {
        format!(
            "::{}",
            Ipv4Addr::new(bytes[12], bytes[13], bytes[14], bytes[15])
        )
    } else {
        address.to_string()
    };
    Ok(Value::from(display))
}
