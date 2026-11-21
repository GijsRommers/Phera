//! `dns_get_record('example.org', DNS_MX)` performs a DNS query on the Tokio reactor.

mod records;
mod wire;

use hickory_resolver::{
    TokioResolver,
    config::ResolveHosts,
    proto::rr::{RData, RecordType},
};
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, PhpArray, Registry, Value, args,
};

use super::checked_argument;

const ANY: i64 = 268435456;
const ALL: i64 = 251721779;
const TYPES: &[(i64, &str, RecordType)] = &[
    (1, "DNS_A", RecordType::A),
    (2, "DNS_NS", RecordType::NS),
    (16, "DNS_CNAME", RecordType::CNAME),
    (32, "DNS_SOA", RecordType::SOA),
    (2048, "DNS_PTR", RecordType::PTR),
    (4096, "DNS_HINFO", RecordType::HINFO),
    (8192, "DNS_CAA", RecordType::CAA),
    (16384, "DNS_MX", RecordType::MX),
    (32768, "DNS_TXT", RecordType::TXT),
    (16777216, "DNS_A6", RecordType::Unknown(38)),
    (33554432, "DNS_SRV", RecordType::SRV),
    (67108864, "DNS_NAPTR", RecordType::NAPTR),
    (134217728, "DNS_AAAA", RecordType::AAAA),
];

pub(super) fn register(registry: &mut Registry) {
    const HOST: Param = Param::new("hostname", ParamType::String);
    const CHECK: &[Param] = &[
        HOST,
        Param::new("type", ParamType::String).with_default(ConstantValue::String("MX")),
    ];
    const NULL_REF: Param = Param::new("weights", ParamType::Mixed)
        .by_ref()
        .with_default(ConstantValue::Null);
    const MX: &[Param] = &[
        HOST,
        Param::new("hosts", ParamType::Mixed).by_ref(),
        NULL_REF,
    ];
    const RECORDS: &[Param] = &[
        HOST,
        Param::new("type", ParamType::Int).with_default(ConstantValue::Int(ANY)),
        Param::new("authoritative_name_servers", ParamType::Mixed)
            .by_ref()
            .with_default(ConstantValue::Null),
        Param::new("additional_records", ParamType::Mixed)
            .by_ref()
            .with_default(ConstantValue::Null),
        Param::new("raw", ParamType::Bool).with_default(ConstantValue::Bool(false)),
    ];
    for entry in [
        FunctionEntry::asynchronous("dns_check_record", CHECK, check_record),
        FunctionEntry::asynchronous("checkdnsrr", CHECK, check_alias),
        FunctionEntry::asynchronous("dns_get_record", RECORDS, record_query),
        FunctionEntry::asynchronous("dns_get_mx", MX, get_mx),
        FunctionEntry::asynchronous("getmxrr", MX, mx_alias),
    ] {
        registry.function(entry);
    }
    for &(mask, name, _) in TYPES {
        registry.constant(name, mask);
    }
    registry.constant("DNS_ANY", ANY);
    registry.constant("DNS_ALL", ALL);
}

pub(super) async fn resolver() -> Option<TokioResolver> {
    tokio::task::spawn_blocking(|| {
        let mut builder = TokioResolver::builder_tokio().ok()?;
        let options = builder.options_mut();
        options.cache_size = 0;
        options.use_hosts_file = ResolveHosts::Never;
        options.preserve_intermediates = true;
        builder.build().ok()
    })
    .await
    .ok()
    .flatten()
}

fn check_record(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(check(context, args, "dns_check_record"))
}
fn check_alias(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(check(context, args, "checkdnsrr"))
}

async fn check(_: &mut dyn Context, arguments: Vec<Value>, function: &str) -> NativeResult {
    let hostname = checked_argument(&arguments, 0, function, "hostname")?;
    if hostname.is_empty() {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #1 ($hostname) must not be empty"
        )));
    }
    let name = checked_argument(&arguments, 1, function, "type")?;
    let name = if args::given(&arguments, 1) {
        name
    } else {
        b"MX"
    };
    let kind = if name.eq_ignore_ascii_case(b"ANY") {
        Some(RecordType::ANY)
    } else {
        TYPES.iter().find_map(|&(_, constant, kind)| {
            constant.as_bytes()[4..]
                .eq_ignore_ascii_case(name)
                .then_some(kind)
        })
    };
    let Some(kind) = kind else {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #2 ($type) must be a valid DNS record type"
        )));
    };
    let Ok(hostname) = std::str::from_utf8(hostname) else {
        return Ok(Value::Bool(false));
    };
    let Some(resolver) = resolver().await else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool(
        resolver
            .lookup(hostname, kind)
            .await
            .is_ok_and(|lookup| !lookup.answers().is_empty()),
    ))
}

fn get_mx(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(mx(context, args, "dns_get_mx"))
}
fn mx_alias(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(mx(context, args, "getmxrr"))
}

async fn mx(_: &mut dyn Context, arguments: Vec<Value>, function: &str) -> NativeResult {
    let hostname = checked_argument(&arguments, 0, function, "hostname")?;
    let mut hosts = PhpArray::new();
    let mut weights = PhpArray::new();
    let mut success = false;
    if let Ok(hostname) = std::str::from_utf8(hostname)
        && !hostname.is_empty()
        && let Some(resolver) = resolver().await
        && let Ok(lookup) = resolver.lookup(hostname, RecordType::MX).await
    {
        for record in lookup.answers() {
            if let RData::MX(record) = &record.data {
                hosts.push(Value::from(records::name(&record.exchange)))?;
                weights.push(Value::Int(record.preference.into()))?;
            }
        }
        success = true;
    }
    set_array(&arguments, 1, hosts);
    set_array(&arguments, 2, weights);
    Ok(Value::Bool(success))
}

fn set_array(arguments: &[Value], index: usize, array: PhpArray) {
    if let Some(reference) = args::reference(arguments, index) {
        *reference.borrow_mut() = Value::from(array);
    }
}

fn record_query(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(query(context, arguments))
}

async fn query(context: &mut dyn Context, arguments: Vec<Value>) -> NativeResult {
    let function = "dns_get_record";
    let hostname = checked_argument(&arguments, 0, function, "hostname")?;
    let raw = args::bool(&arguments, 4).unwrap_or(false);
    let mask = args::int(&arguments, 1).unwrap_or(ANY);
    set_array(&arguments, 2, PhpArray::new());
    set_array(&arguments, 3, PhpArray::new());
    let kinds = query_types(mask, raw)?;
    if kinds.is_empty() {
        return Ok(Value::from(PhpArray::new()));
    }
    if hostname.is_empty() {
        context.warning(format!("{function}(): DNS Query failed"));
        return Ok(Value::Bool(false));
    }
    let Ok(hostname) = std::str::from_utf8(hostname) else {
        return Ok(Value::from(PhpArray::new()));
    };
    if raw {
        let Some([answer, authority, additional]) = wire::lookup(hostname, kinds[0]).await else {
            context.warning(format!("{function}(): DNS Query failed"));
            return Ok(Value::Bool(false));
        };
        set_array(&arguments, 2, authority);
        set_array(&arguments, 3, additional);
        return Ok(Value::from(answer));
    }
    let Some(resolver) = resolver().await else {
        context.warning(format!("{function}(): DNS Query failed"));
        return Ok(Value::Bool(false));
    };
    let mut answer = PhpArray::new();
    let mut authority = PhpArray::new();
    let mut additional = PhpArray::new();
    for kind in kinds {
        match resolver.lookup(hostname, kind).await {
            Ok(lookup) => {
                let matching = lookup
                    .answers()
                    .iter()
                    .filter(|record| kind == RecordType::ANY || record.data.record_type() == kind);
                records::append(&mut answer, matching)?;
                records::append(&mut authority, lookup.authorities().iter())?;
                records::append(&mut additional, lookup.additionals().iter())?;
            }
            Err(error) if error.is_no_records_found() => (),
            Err(_) => {
                context.warning(format!("{function}(): DNS Query failed"));
                return Ok(Value::Bool(false));
            }
        }
    }
    set_array(&arguments, 2, authority);
    set_array(&arguments, 3, additional);
    Ok(Value::from(answer))
}

fn query_types(mask: i64, raw: bool) -> Result<Vec<RecordType>, NativeError> {
    if raw {
        let Ok(code) = u16::try_from(mask) else {
            return Err(invalid_raw_type());
        };
        if code == 0 {
            return Err(invalid_raw_type());
        }
        return Ok(vec![RecordType::from(code)]);
    }
    if mask == ANY {
        return Ok(vec![RecordType::ANY]);
    }
    if mask & !ALL != 0 {
        return Err(NativeError::value_error(
            "dns_get_record(): Argument #2 ($type) must be a DNS_* constant",
        ));
    }
    Ok(TYPES
        .iter()
        .filter_map(|&(bit, _, kind)| (mask & bit != 0).then_some(kind))
        .collect())
}

fn invalid_raw_type() -> NativeError {
    NativeError::value_error(
        "dns_get_record(): Argument #2 ($type) must be between 1 and 65535 when argument #5 ($raw) is true",
    )
}
