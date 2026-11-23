//! `dns_get_record($host, 15, $ns, $extra, true)` keeps the response's raw RDATA bytes.

use std::sync::Arc;

use hickory_resolver::{
    NameServerPool, PoolContext, TlsConfig,
    net::{
        runtime::TokioRuntimeProvider,
        xfer::{DnsHandle, FirstAnswer},
    },
    proto::{
        op::{DnsRequestOptions, Query},
        rr::{Name, RecordType},
        serialize::binary::{BinDecodable, BinDecoder},
    },
    system_conf::read_system_conf,
};
use phera_extension_api::{ArrayKey, PhpArray, Value};

pub(super) async fn lookup(host: &str, kind: RecordType) -> Option<[PhpArray; 3]> {
    let (config, options) = tokio::task::spawn_blocking(read_system_conf)
        .await
        .ok()?
        .ok()?;
    let name = Name::from_ascii(host).ok()?;
    let mut names = vec![name.clone()];
    if !name.is_fqdn() {
        let suffixes = config.search.iter().chain(config.domain.iter());
        let expanded = suffixes.filter_map(|suffix| name.clone().append_domain(suffix).ok());
        if usize::from(name.num_labels().saturating_sub(1)) >= options.ndots {
            names.extend(expanded);
        } else {
            names.splice(0..0, expanded);
        }
    }
    let pool = NameServerPool::from_config(
        config.name_servers,
        Arc::new(PoolContext::new(options, TlsConfig::new().ok()?)),
        TokioRuntimeProvider::default(),
    );
    for candidate in names {
        match pool
            .lookup(Query::query(candidate, kind), DnsRequestOptions::default())
            .first_answer()
            .await
        {
            Ok(response) => return decode(response.as_buffer(), kind),
            Err(error) if error.is_no_records_found() => (),
            Err(_) => return None,
        }
    }
    Some(std::array::from_fn(|_| PhpArray::new()))
}

fn decode(packet: &[u8], requested: RecordType) -> Option<[PhpArray; 3]> {
    fn count(decoder: &mut BinDecoder<'_>) -> Option<u16> {
        Some(decoder.read_u16().ok()?.unverified())
    }

    let mut decoder = BinDecoder::new(packet);
    decoder.read_slice(4).ok()?;
    let questions = count(&mut decoder)?;
    let counts = [
        count(&mut decoder)?,
        count(&mut decoder)?,
        count(&mut decoder)?,
    ];
    for _ in 0..questions {
        Query::read(&mut decoder).ok()?;
    }
    let mut sections = std::array::from_fn(|_| PhpArray::new());
    for (index, (section, total)) in sections.iter_mut().zip(counts).enumerate() {
        for _ in 0..total {
            let host = Name::read(&mut decoder).ok()?;
            let kind = RecordType::read(&mut decoder).ok()?;
            decoder.read_u16().ok()?;
            let ttl = decoder.read_u32().ok()?.unverified();
            let length = usize::from(count(&mut decoder)?);
            let data = decoder.read_slice(length).ok()?.unverified();
            if kind == RecordType::OPT
                || (index == 0 && requested != RecordType::ANY && kind != requested)
            {
                continue;
            }
            let mut record = PhpArray::new();
            for (key, value) in [
                ("host", Value::from(super::records::name(&host))),
                ("class", Value::from("IN")),
                ("ttl", Value::Int(ttl.into())),
                ("type", Value::Int(u16::from(kind).into())),
                ("data", Value::from(data.to_vec())),
            ] {
                record.insert(ArrayKey::str(key), value);
            }
            section.push(Value::from(record)).ok()?;
        }
    }
    Some(sections)
}
