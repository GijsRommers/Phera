//! `stream_context_create(['ssl' => ['verify_peer' => true]])` selects a trust policy.

use std::{io, path::PathBuf};

use phera_extension_api::{ArrayKey, Context, PhpArray, Value, WorkingDirectory, value::to_bool};

#[derive(Clone, PartialEq, Eq)]
pub(in crate::socket) struct Options {
    pub verify_peer: bool,
    pub verify_name: bool,
    pub allow_self_signed: bool,
    pub sni: bool,
    pub peer_name: Option<String>,
    pub cafile: Option<PathBuf>,
    pub capath: Option<PathBuf>,
    pub verify_depth: Option<usize>,
    pub fingerprints: Vec<(Option<Vec<u8>>, Vec<u8>)>,
    pub local_cert: Option<PathBuf>,
    pub local_pk: Option<PathBuf>,
    pub passphrase: Option<String>,
    pub method: Option<i64>,
}

impl Options {
    pub fn in_context(
        context: &mut dyn Context,
        options: &PhpArray,
        client: bool,
    ) -> io::Result<Self> {
        let mut options = Self::parse(options, client)?;
        for path in [
            &mut options.cafile,
            &mut options.capath,
            &mut options.local_cert,
            &mut options.local_pk,
        ]
        .into_iter()
        .flatten()
        {
            if path.is_relative() {
                *path = context
                    .state()
                    .get_or_default::<WorkingDirectory>()
                    .resolve(path)?;
            }
        }
        Ok(options)
    }

    pub fn parse(options: &PhpArray, client: bool) -> io::Result<Self> {
        let get = |key: &str| {
            options
                .get(&ArrayKey::str(key))
                .map(|value| value.deref().clone())
        };
        let boolean = |key, default| get(key).map_or(default, |value| to_bool(&value));
        let string = |key| match get(key) {
            Some(Value::String(value)) => Some(value.to_string_lossy()),
            _ => None,
        };
        let fingerprints = match get("peer_fingerprint") {
            Some(Value::String(value)) => vec![(None, value.as_bytes().to_vec())],
            Some(Value::Array(values)) => values
                .iter()
                .map(|(key, value)| match (key, &*value.deref()) {
                    (ArrayKey::String(key), Value::String(value)) => {
                        Ok((Some(key.as_bytes().to_vec()), value.as_bytes().to_vec()))
                    }
                    _ => Err(super::invalid("Invalid SSL peer fingerprint")),
                })
                .collect::<io::Result<_>>()?,
            Some(Value::Null) | None => Vec::new(),
            _ => return Err(super::invalid("Invalid SSL peer fingerprint")),
        };
        Ok(Self {
            verify_peer: boolean("verify_peer", client),
            verify_name: boolean("verify_peer_name", client),
            allow_self_signed: boolean("allow_self_signed", false),
            sni: boolean("SNI_enabled", true),
            peer_name: string("peer_name"),
            cafile: string("cafile").map(PathBuf::from),
            capath: string("capath").map(PathBuf::from),
            verify_depth: match get("verify_depth") {
                Some(Value::Int(value)) => Some(value.max(0) as usize),
                _ => None,
            },
            fingerprints,
            local_cert: string("local_cert").map(PathBuf::from),
            local_pk: string("local_pk").map(PathBuf::from),
            passphrase: string("passphrase"),
            method: match get("crypto_method") {
                Some(Value::Int(value)) => Some(value),
                _ => None,
            },
        })
    }
}
