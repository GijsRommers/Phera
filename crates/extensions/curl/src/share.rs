//! `curl_share_setopt($share, CURLSHOPT_SHARE, CURL_LOCK_DATA_COOKIE)` shares cookies.

use std::{
    cell::RefCell,
    collections::VecDeque,
    net::SocketAddr,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Instant,
};

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

use crate::{classes, constants::ids::*, cookies::CookieJar, easy};

pub(crate) type ShareHandle = Rc<RefCell<Share>>;

#[derive(Default)]
pub(crate) struct DnsCache {
    entries: VecDeque<DnsEntry>,
}

struct DnsEntry {
    host: String,
    port: u16,
    time: Instant,
    addresses: Vec<SocketAddr>,
}

impl DnsCache {
    pub(crate) fn get(&self, host: &str, port: u16) -> Option<Vec<SocketAddr>> {
        self.entries
            .iter()
            .rev()
            .find(|entry| {
                entry.host == host && entry.port == port && entry.time.elapsed().as_secs() < 60
            })
            .map(|entry| entry.addresses.clone())
    }

    pub(crate) fn put(&mut self, host: &str, port: u16, addresses: &[SocketAddr]) {
        self.entries
            .retain(|entry| entry.host != host || entry.port != port);
        if self.entries.len() >= 128 {
            self.entries.pop_front();
        }
        self.entries.push_back(DnsEntry {
            host: host.to_owned(),
            port,
            time: Instant::now(),
            addresses: addresses.to_vec(),
        });
    }
}

pub(crate) type SharedDns = Arc<Mutex<DnsCache>>;

#[derive(Default)]
pub(crate) struct Share {
    pub(crate) cookies: Option<CookieJar>,
    pub(crate) dns: Option<SharedDns>,
    pub(crate) attachments: usize,
    error: i64,
}

const HANDLE: Param = Param::new("share_handle", ParamType::Class("CurlShareHandle"));

pub(crate) fn register(registry: &mut Registry) {
    const FUNCTIONS: &[FunctionEntry] = &[
        FunctionEntry::sync("curl_share_init", &[], init),
        FunctionEntry::sync("curl_share_close", &[HANDLE], close),
        FunctionEntry::sync("curl_share_errno", &[HANDLE], errno),
        FunctionEntry::sync(
            "curl_share_strerror",
            &[easy::ERROR],
            crate::error::curl_share_strerror,
        ),
        FunctionEntry::sync(
            "curl_share_setopt",
            &[
                HANDLE,
                Param::new("option", ParamType::Int),
                Param::new("value", ParamType::Mixed),
            ],
            setopt,
        ),
        FunctionEntry::sync(
            "curl_share_init_persistent",
            &[Param::new("share_options", ParamType::Array)],
            persistent,
        ),
    ];
    for &entry in FUNCTIONS {
        registry.function(entry);
    }
}

fn init(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    classes::create(context, "CurlShareHandle", ShareHandle::default())
}

fn persistent(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let mut options = Vec::new();
    if let Some(array) = args::array(values, 0) {
        for value in array.values() {
            let option = match &*value.deref() {
                Value::Int(CURL_LOCK_DATA_COOKIE) => {
                    return Err(NativeError::value_error(concat!(
                        "curl_share_init_persistent(): Argument #1 ($share_options) must not ",
                        "contain CURL_LOCK_DATA_COOKIE because sharing cookies across PHP requests is unsafe"
                    )));
                }
                Value::Int(option)
                    if matches!(*option, CURL_LOCK_DATA_DNS | CURL_LOCK_DATA_CONNECT) =>
                {
                    *option
                }
                Value::Int(CURL_LOCK_DATA_SSL_SESSION | CURL_LOCK_DATA_PSL) => {
                    return Err(NativeError::error(
                        "Could not construct persistent cURL share handle: Feature not available",
                    ));
                }
                _ => {
                    return Err(NativeError::value_error(concat!(
                        "curl_share_init_persistent(): Argument #1 ($share_options) must contain ",
                        "only CURL_LOCK_DATA_* constants"
                    )));
                }
            };
            options.push(option);
        }
    }
    options.sort_unstable();
    options.dedup();
    thread_local! {
        static DNS: SharedDns = SharedDns::default();
    }
    let share = Share {
        dns: options
            .contains(&CURL_LOCK_DATA_DNS)
            .then(|| DNS.with(Clone::clone)),
        ..Share::default()
    };
    let value = classes::create(
        context,
        "CurlSharePersistentHandle",
        Rc::new(RefCell::new(share)),
    )?;
    if let Value::Object(object) = &value {
        let mut list = PhpArray::new();
        for option in options {
            list.push(Value::Int(option))?;
        }
        object.set(ArrayKey::str("options"), Value::from(list));
    }
    Ok(value)
}

fn close(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    easy::deprecated_close(context, "curl_share_close");
    Ok(Value::Null)
}

fn errno(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let share: ShareHandle = classes::state(easy::object(values, 0)?)?;
    Ok(Value::Int(share.borrow().error))
}

fn setopt(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let share: ShareHandle = classes::state(easy::object(values, 0)?)?;
    let option = args::int(values, 1).unwrap_or_default();
    if !matches!(option, CURLSHOPT_SHARE | CURLSHOPT_UNSHARE) {
        return Err(NativeError::value_error(
            "curl_share_setopt(): Argument #2 ($option) is not a valid cURL share option",
        ));
    }
    let data = phera_extension_api::value::to_int(args::value(values, 2), context.diagnostics())?;
    let mut share = share.borrow_mut();
    let enabled = option == CURLSHOPT_SHARE;
    share.error = match (share.attachments != 0, data) {
        (true, _) => 2,
        (_, CURL_LOCK_DATA_COOKIE) => {
            if enabled {
                share.cookies.get_or_insert_with(CookieJar::default);
            } else {
                share.cookies = None;
            }
            0
        }
        (_, CURL_LOCK_DATA_DNS) => {
            if enabled {
                share.dns.get_or_insert_with(SharedDns::default);
            } else {
                share.dns = None;
            }
            0
        }
        (_, CURL_LOCK_DATA_CONNECT) => 0,
        _ => 5,
    };
    Ok(Value::Bool(share.error == 0))
}
