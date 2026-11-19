//! `curl_init($url)` creates one reusable transfer configuration.

use std::{cell::RefCell, rc::Rc};

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Object, Param,
    ParamType, Registry, Value, args,
};

use crate::{classes, cookies::CookieJar, error, info::Info, options::Settings};

pub(crate) type Handle = Rc<RefCell<Easy>>;

#[derive(Default)]
pub(crate) struct Easy {
    pub(crate) settings: Settings,
    pub(crate) info: Info,
    pub(crate) error: Option<error::TransferError>,
    pub(crate) content: Option<Vec<u8>>,
    pub(crate) active: bool,
    pub(crate) paused: bool,
    pub(crate) multi: Option<std::rc::Weak<RefCell<crate::multi::Multi>>>,
    pub(crate) cookie_jar: CookieJar,
    pub(crate) share: Option<crate::share::ShareHandle>,
}

impl Drop for Easy {
    fn drop(&mut self) {
        self.attach_share(None);
    }
}

impl Easy {
    pub(crate) fn attach_share(&mut self, share: Option<crate::share::ShareHandle>) {
        if let Some(previous) = self.share.take() {
            previous.borrow_mut().attachments -= 1;
        }
        if let Some(share) = &share {
            share.borrow_mut().attachments += 1;
        }
        self.share = share;
    }

    pub(crate) fn cookies(&self) -> CookieJar {
        self.share
            .as_ref()
            .and_then(|share| share.borrow().cookies.clone())
            .unwrap_or_else(|| self.cookie_jar.clone())
    }

    pub(crate) fn cookie_engine(&self) -> bool {
        !self.settings.cookie_files.is_empty()
            || self.settings.cookie_jar.is_some()
            || self.cookies().borrow().enabled
    }
}

pub(crate) const HANDLE: Param = Param::new("handle", ParamType::Class("CurlHandle"));
const OPTION: Param = Param::new("option", ParamType::Int);
const VALUE: Param = Param::new("value", ParamType::Mixed);
pub(crate) const ERROR: Param = Param::new("error_code", ParamType::Int);

pub(crate) fn register(registry: &mut Registry) {
    const FUNCTIONS: &[FunctionEntry] = &[
        FunctionEntry::sync(
            "curl_init",
            &[Param::new("url", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null)],
            init,
        ),
        FunctionEntry::asynchronous("curl_exec", &[HANDLE], exec),
        FunctionEntry::asynchronous(
            "curl_setopt",
            &[HANDLE, OPTION, VALUE],
            crate::options::set_option,
        ),
        FunctionEntry::asynchronous(
            "curl_setopt_array",
            &[HANDLE, Param::new("options", ParamType::Array)],
            crate::options::set_array,
        ),
        FunctionEntry::sync(
            "curl_getinfo",
            &[HANDLE, OPTION.nullable().with_default(ConstantValue::Null)],
            crate::info::get,
        ),
        FunctionEntry::sync("curl_error", &[HANDLE], last_error),
        FunctionEntry::sync("curl_errno", &[HANDLE], errno),
        FunctionEntry::sync("curl_strerror", &[ERROR], error::curl_strerror),
        FunctionEntry::sync("curl_close", &[HANDLE], close),
        FunctionEntry::sync("curl_reset", &[HANDLE], reset),
        FunctionEntry::sync("curl_copy_handle", &[HANDLE], copy_handle),
        FunctionEntry::sync("curl_version", &[], crate::version::version),
        FunctionEntry::sync(
            "curl_escape",
            &[HANDLE, Param::new("string", ParamType::String)],
            escape,
        ),
        FunctionEntry::sync(
            "curl_unescape",
            &[HANDLE, Param::new("string", ParamType::String)],
            unescape,
        ),
        FunctionEntry::sync("curl_upkeep", &[HANDLE], upkeep),
        FunctionEntry::sync(
            "curl_pause",
            &[HANDLE, Param::new("flags", ParamType::Int)],
            pause,
        ),
    ];
    for &entry in FUNCTIONS {
        registry.function(entry);
    }
}

pub(crate) fn handle(values: &[Value]) -> Result<Handle, NativeError> {
    classes::state(object(values, 0)?)
}

pub(crate) fn object(values: &[Value], index: usize) -> Result<&Rc<Object>, NativeError> {
    args::object(values, index).ok_or_else(|| NativeError::error("cURL handle is unavailable"))
}

fn init(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let mut easy = Easy::default();
    if let Some(url) = args::string(values, 0) {
        if url.as_bytes().contains(&0) {
            return Err(NativeError::value_error(
                "curl_init(): cURL option must not contain any null bytes",
            ));
        }
        easy.settings.url = Some(String::from_utf8_lossy(url.as_bytes()).into_owned());
    }
    classes::create(context, "CurlHandle", Rc::new(RefCell::new(easy)))
}

pub(crate) fn copy(object: &Object) -> Result<Handle, NativeError> {
    let state: Handle = classes::state(object)?;
    let original = state.borrow();
    let mut copy = Easy::default();
    copy.settings = original.settings.clone();
    copy.attach_share(original.share.clone());
    Ok(Rc::new(RefCell::new(copy)))
}

fn copy_handle(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    classes::create(context, "CurlHandle", copy(object(values, 0)?)?)
}

fn last_error(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    Ok(Value::from(
        handle(values)?
            .borrow()
            .error
            .as_ref()
            .map_or_else(String::new, |error| error.message.clone()),
    ))
}

fn errno(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    Ok(Value::Int(
        handle(values)?
            .borrow()
            .error
            .as_ref()
            .map_or(0, |error| error.code),
    ))
}

pub(crate) fn deprecated_close(context: &mut dyn Context, function: &str) {
    context.deprecated(format!(
        "Function {function}() is deprecated since 8.5, as it has no effect since PHP 8.0"
    ));
}

fn close(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    deprecated_close(context, "curl_close");
    if handle(values)?.borrow().active {
        return Err(NativeError::error(
            "curl_close(): Attempt to close cURL handle from a callback",
        ));
    }
    Ok(Value::Null)
}

fn reset(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let handle = handle(values)?;
    let mut easy = handle.borrow_mut();
    if easy.active {
        return Err(NativeError::error(
            "curl_reset(): Attempt to reset cURL handle from a callback",
        ));
    }
    easy.settings = Settings::default();
    easy.info = Info::default();
    easy.content = None;
    easy.attach_share(None);
    Ok(Value::Null)
}

fn escape(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let bytes = args::bytes(values, 1);
    let mut output = Vec::with_capacity(bytes.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &byte in bytes {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            output.push(byte);
        } else {
            output.extend_from_slice(&[
                b'%',
                HEX[usize::from(byte >> 4)],
                HEX[usize::from(byte & 15)],
            ]);
        }
    }
    Ok(Value::from(output))
}

fn unescape(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let bytes = args::bytes(values, 1);
    let mut remaining = bytes;
    let mut output = Vec::with_capacity(bytes.len());
    while let [first, rest @ ..] = remaining {
        if let [b'%', a, b, tail @ ..] = remaining
            && let (Some(a), Some(b)) = (char::from(*a).to_digit(16), char::from(*b).to_digit(16))
        {
            output.push((a * 16 + b) as u8);
            remaining = tail;
        } else {
            output.push(*first);
            remaining = rest;
        }
    }
    Ok(Value::from(output))
}

fn upkeep(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    handle(values)?;
    Ok(Value::Bool(true))
}

fn pause(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let handle = handle(values)?;
    let mut easy = handle.borrow_mut();
    if !easy.active {
        return Ok(Value::Int(43));
    }
    easy.paused = args::int(values, 1).unwrap_or_default() & 1 != 0;
    Ok(Value::Int(0))
}

fn exec(context: &mut dyn Context, values: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = object(&values, 0)?.clone();
        let handle: Handle = classes::state(&object)?;
        if handle.borrow().active
            || handle
                .borrow()
                .multi
                .as_ref()
                .and_then(|owner| owner.upgrade())
                .is_some()
        {
            return Err(NativeError::error(
                "curl_exec(): Attempt to perform cURL handle from a callback",
            ));
        }
        let mut transfer = crate::transfer::Transfer::start(context, object, handle).await?;
        while transfer.step(context).await?.is_pending() {}
        Ok(transfer.result())
    })
}
