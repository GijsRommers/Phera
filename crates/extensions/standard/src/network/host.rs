//! `gethostbyname('localhost')` and `gethostbynamel('localhost')` resolve IPv4 addresses.

use std::net::{IpAddr, Ipv4Addr};

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, PhpArray, Registry, Value,
};

use super::checked_argument;

pub(super) fn register(registry: &mut Registry) {
    const HOST: &[Param] = &[Param::new("hostname", ParamType::String)];
    registry.function(FunctionEntry::asynchronous("gethostbyname", HOST, by_name));
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
