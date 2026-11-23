//! A DNS record exposes the same fields as PHP's `dns_get_record()`.

use hickory_resolver::proto::{
    rr::{Name, RData, Record},
    serialize::binary::{BinDecodable, BinDecoder},
};
use phera_extension_api::{ArrayKey, NativeError, PhpArray, Value};

pub(super) fn name(name: &Name) -> String {
    let presentation = name.to_ascii();
    presentation
        .strip_suffix('.')
        .unwrap_or(&presentation)
        .to_owned()
}

pub(super) fn append<'a>(
    output: &mut PhpArray,
    records: impl Iterator<Item = &'a Record>,
) -> Result<(), NativeError> {
    for record in records {
        if let Some(array) = fields(record)? {
            output.push(Value::from(array))?;
        }
    }
    Ok(())
}

fn fields(record: &Record) -> Result<Option<PhpArray>, NativeError> {
    let mut array = PhpArray::new();
    let mut insert = |key, value| array.insert(ArrayKey::str(key), value);
    insert("host", Value::from(name(&record.name)));
    insert("class", Value::from("IN"));
    insert("ttl", Value::Int(record.ttl.into()));
    let kind = record.data.record_type();
    let kind_name = if u16::from(kind) == 38 {
        "A6"
    } else {
        <&str>::from(kind)
    };
    insert("type", Value::from(kind_name));
    let mut integer = |key, value| insert(key, Value::Int(value));
    match &record.data {
        RData::A(address) => {
            insert("ip", Value::from(address.0.to_string()));
        }
        RData::AAAA(address) => {
            insert("ipv6", Value::from(address.0.to_string()));
        }
        RData::CNAME(record) => {
            insert("target", Value::from(name(&record.0)));
        }
        RData::NS(record) => {
            insert("target", Value::from(name(&record.0)));
        }
        RData::PTR(record) => {
            insert("target", Value::from(name(&record.0)));
        }
        RData::MX(record) => {
            integer("pri", record.preference.into());
            insert("target", Value::from(name(&record.exchange)));
        }
        RData::HINFO(record) => {
            insert("cpu", Value::from(record.cpu.to_vec()));
            insert("os", Value::from(record.os.to_vec()));
        }
        RData::CAA(record) => {
            let flags = record.reserved_flags | if record.issuer_critical { 128 } else { 0 };
            integer("flags", flags.into());
            insert("tag", Value::from(record.tag.as_str()));
            insert("value", Value::from(record.value.clone()));
        }
        RData::TXT(record) => {
            let mut text = Vec::new();
            let mut entries = PhpArray::new();
            for part in &record.txt_data {
                text.extend_from_slice(part);
                entries.push(Value::from(part.to_vec()))?;
            }
            insert("txt", Value::from(text));
            insert("entries", Value::from(entries));
        }
        RData::SOA(record) => {
            insert("mname", Value::from(name(&record.mname)));
            insert("rname", Value::from(name(&record.rname)));
            for (key, value) in [
                ("serial", record.serial.into()),
                ("refresh", record.refresh.into()),
                ("retry", record.retry.into()),
                ("expire", record.expire.into()),
                ("minimum-ttl", record.minimum.into()),
            ] {
                insert(key, Value::Int(value));
            }
        }
        RData::SRV(record) => {
            integer("pri", record.priority.into());
            integer("weight", record.weight.into());
            integer("port", record.port.into());
            insert("target", Value::from(name(&record.target)));
        }
        RData::NAPTR(record) => {
            integer("order", record.order.into());
            integer("pref", record.preference.into());
            insert("flags", Value::from(record.flags.to_vec()));
            insert("services", Value::from(record.services.to_vec()));
            insert("regex", Value::from(record.regexp.to_vec()));
            insert("replacement", Value::from(name(&record.replacement)));
        }
        RData::Unknown { code, rdata } if u16::from(*code) == 38 => {
            let Some((&prefix, bytes)) = rdata.anything.split_first() else {
                return Ok(None);
            };
            if prefix > 128 {
                return Ok(None);
            }
            let length = (128 - usize::from(prefix)).div_ceil(8);
            let Some(suffix) = bytes.get(..length) else {
                return Ok(None);
            };
            let mut address = [0; 16];
            address[16 - length..].copy_from_slice(suffix);
            integer("masklen", prefix.into());
            if prefix < 128 {
                insert(
                    "ipv6",
                    Value::from(std::net::Ipv6Addr::from(address).to_string()),
                );
            }
            if prefix > 0
                && let Ok(domain) = Name::read(&mut BinDecoder::new(&bytes[length..]))
            {
                insert("chain", Value::from(name(&domain)));
            }
        }
        _ => return Ok(None),
    }
    Ok(Some(array))
}
