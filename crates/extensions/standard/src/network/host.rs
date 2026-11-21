//! `gethostbyname('localhost')` and `gethostbynamel('localhost')` resolve IPv4 addresses.

use std::net::{IpAddr, Ipv4Addr};

use hickory_resolver::proto::rr::{Name, RData};

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, PhpArray, Registry, Value,
};

use super::checked_argument;

pub(super) fn register(registry: &mut Registry) {
    const HOST: &[Param] = &[Param::new("hostname", ParamType::String)];
    registry.function(FunctionEntry::asynchronous("gethostbyname", HOST, by_name));
    registry.function(FunctionEntry::asynchronous(
        "gethostbyaddr",
        const { &[Param::new("ip", ParamType::String)] },
        by_address,
    ));
    registry.function(FunctionEntry::asynchronous(
        "gethostbynamel",
        HOST,
        all_by_name,
    ));
}

fn by_name(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(lookup(context, arguments, "gethostbyname", Output::First))
}

fn all_by_name(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(lookup(context, arguments, "gethostbynamel", Output::All))
}

enum Output {
    First,
    All,
}

async fn lookup(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
    output: Output,
) -> NativeResult {
    let hostname = checked_argument(&arguments, 0, function, "hostname")?;
    let fallback = || match output {
        Output::First => Value::from(hostname.to_vec()),
        Output::All => Value::Bool(false),
    };
    if hostname.len() > 255 {
        context.warning(format!(
            "{function}(): Host name cannot be longer than 255 characters"
        ));
        return Ok(fallback());
    }
    let Ok(hostname) = std::str::from_utf8(hostname) else {
        return Ok(fallback());
    };
    if hostname.is_empty() {
        return Ok(fallback());
    }
    let Ok(addresses) = tokio::net::lookup_host((hostname, 0)).await else {
        return Ok(fallback());
    };
    let mut unique = Vec::<Ipv4Addr>::new();
    for address in addresses {
        let IpAddr::V4(address) = address.ip() else {
            continue;
        };
        if matches!(output, Output::First) {
            return Ok(Value::from(address.to_string()));
        }
        if !unique.contains(&address) {
            unique.push(address);
        }
    }
    if unique.is_empty() {
        return Ok(fallback());
    }
    let mut values = PhpArray::new();
    for address in unique {
        values.push(Value::from(address.to_string()))?;
    }
    Ok(Value::from(values))
}

fn by_address(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let bytes = checked_argument(&arguments, 0, "gethostbyaddr", "ip")?;
        let address = std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| text.parse::<IpAddr>().ok());
        let Some(address) = address else {
            context.warning("gethostbyaddr(): Address is not a valid IPv4 or IPv6 address".into());
            return Ok(Value::Bool(false));
        };
        if let Ok(hosts) = tokio::fs::read_to_string("/etc/hosts").await
            && let Some(name) = hosts.lines().find_map(|line| {
                let mut words = line.split('#').next()?.split_ascii_whitespace();
                let ip = words.next()?.parse::<IpAddr>().ok()?;
                (ip == address).then(|| words.next()).flatten()
            })
        {
            return Ok(Value::from(name));
        }
        if let Some(resolver) = super::dns::resolver().await
            && let Ok(lookup) = resolver.reverse_lookup(Name::from(address)).await
            && let Some(name) = lookup
                .answers()
                .iter()
                .find_map(|record| match &record.data {
                    RData::PTR(ptr) => Some(ptr.0.to_ascii()),
                    _ => None,
                })
        {
            return Ok(Value::from(name.trim_end_matches('.')));
        }
        Ok(Value::from(bytes.to_vec()))
    })
}
