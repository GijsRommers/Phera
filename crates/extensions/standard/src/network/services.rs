//! `getservbyname('http', 'tcp')` and `getprotobynumber(6)` consult system tables.

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Registry, Value, args,
};

use super::checked_argument;

pub(super) fn register(registry: &mut Registry) {
    const PROTOCOL: Param = Param::new("protocol", ParamType::String);
    const NAME: &[Param] = &[Param::new("service", ParamType::String), PROTOCOL];
    const PORT: &[Param] = &[Param::new("port", ParamType::Int), PROTOCOL];
    for entry in [
        FunctionEntry::asynchronous("getservbyname", NAME, service_name),
        FunctionEntry::asynchronous("getservbyport", PORT, service_port),
        FunctionEntry::asynchronous("getprotobyname", const { &[PROTOCOL] }, protocol_name),
        FunctionEntry::asynchronous(
            "getprotobynumber",
            const { &[Param::new("protocol", ParamType::Int)] },
            protocol_number,
        ),
    ] {
        registry.function(entry);
    }
}

fn service_name(_: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "getservbyname";
        let name = checked_argument(&arguments, 0, function, "service")?;
        let protocol = checked_argument(&arguments, 1, function, "protocol")?;
        Ok(services(protocol, Match::Name(name)).await)
    })
}

fn service_port(_: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let protocol = checked_argument(&arguments, 1, "getservbyport", "protocol")?;
        let port = args::int(&arguments, 0).unwrap_or_default() as u16;
        Ok(services(protocol, Match::Number(i64::from(port))).await)
    })
}

fn protocol_name(_: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = checked_argument(&arguments, 0, "getprotobyname", "protocol")?;
        Ok(protocols(Match::Name(name)).await)
    })
}

fn protocol_number(_: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let number = args::int(&arguments, 0).unwrap_or_default() as i32;
        Ok(protocols(Match::Number(i64::from(number))).await)
    })
}

enum Match<'a> {
    Name(&'a [u8]),
    Number(i64),
}

fn entries(contents: &str) -> impl Iterator<Item = (&str, &str, impl Iterator<Item = &str>)> {
    contents.lines().filter_map(|line| {
        let mut fields = line.split('#').next()?.split_ascii_whitespace();
        Some((fields.next()?, fields.next()?, fields))
    })
}

async fn services(protocol: &[u8], target: Match<'_>) -> Value {
    let Ok(contents) = tokio::fs::read_to_string("/etc/services").await else {
        return Value::Bool(false);
    };
    for (name, spec, mut aliases) in entries(&contents) {
        let Some((port, kind)) = spec.split_once('/') else {
            continue;
        };
        if kind.as_bytes() != protocol {
            continue;
        }
        let Ok(port) = port.parse::<u16>().map(i64::from) else {
            continue;
        };
        match target {
            Match::Name(requested)
                if name.as_bytes() == requested || aliases.any(|a| a.as_bytes() == requested) =>
            {
                return Value::Int(port);
            }
            Match::Number(requested) if port == requested => return Value::from(name),
            _ => (),
        }
    }
    Value::Bool(false)
}

async fn protocols(target: Match<'_>) -> Value {
    let Ok(contents) = tokio::fs::read_to_string("/etc/protocols").await else {
        return Value::Bool(false);
    };
    for (name, number, mut aliases) in entries(&contents) {
        let Ok(number) = number.parse::<i64>() else {
            continue;
        };
        match target {
            Match::Name(requested)
                if name.as_bytes() == requested || aliases.any(|a| a.as_bytes() == requested) =>
            {
                return Value::Int(number);
            }
            Match::Number(requested) if number == requested => return Value::from(name),
            _ => (),
        }
    }
    Value::Bool(false)
}
