//! The web SAPI exposes incoming headers and the name of the current SAPI.
//!
//! `getallheaders()['Host']` reads the request's Host header on the web server.

use std::collections::{HashMap, hash_map::Entry};

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeResult, PhpArray, Registry, Value,
};

#[derive(Clone, Copy, Default)]
pub enum Sapi {
    #[default]
    Cli,
    Web,
}

impl Sapi {
    pub fn name(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Web => "phera",
        }
    }
}

#[derive(Default)]
pub struct RequestHeaders(pub Vec<(String, Vec<u8>)>);

pub fn register_web(registry: &mut Registry) {
    for function in ["getallheaders", "apache_request_headers"] {
        registry.function(FunctionEntry::sync(function, &[], request_headers));
    }
    registry.constant("PHP_SAPI", Sapi::Web.name());
}

pub(super) fn sapi_name(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::from(sapi(context).name()))
}

pub(super) fn sapi(context: &mut dyn Context) -> Sapi {
    context.state().get::<Sapi>().copied().unwrap_or_default()
}

fn request_headers(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    let mut result = PhpArray::new();
    if let Some(headers) = context.state().get::<RequestHeaders>() {
        let mut values = HashMap::<String, Vec<u8>>::new();
        for (name, value) in &headers.0 {
            let lower = name.to_ascii_lowercase();
            let combined = match values.entry(lower) {
                Entry::Occupied(entry) => {
                    let combined = entry.into_mut();
                    if name.eq_ignore_ascii_case("set-cookie") {
                        combined.clear();
                    } else {
                        combined.extend_from_slice(b", ");
                    }
                    combined.extend_from_slice(value);
                    combined
                }
                Entry::Vacant(entry) => entry.insert(value.clone()),
            };
            result.insert(
                ArrayKey::str(name.as_str()),
                Value::from(combined.as_slice()),
            );
        }
    }
    Ok(Value::from(result))
}
