//! `stream_wrapper_register('virtual', VirtualFile::class)` changes this request's schemes.

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, PhpArray, Registry, Value, args,
};

use crate::request::files;

const BUILTINS: &[&[u8]] = &[b"php", b"file", b"data", b"compress.zlib"];
const PROTOCOL: Param = Param::new("protocol", ParamType::String);
const FLAGS: Param = Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(0));
const REGISTER: &[Param] = &[PROTOCOL, Param::new("class", ParamType::String), FLAGS];

pub const USE_PATH: i64 = 1;
pub const REPORT_ERRORS: i64 = 8;
pub const IS_URL: i64 = 1;

pub fn register(registry: &mut Registry) {
    for function in [
        FunctionEntry::asynchronous("stream_wrapper_register", REGISTER, register_function),
        FunctionEntry::asynchronous("stream_register_wrapper", REGISTER, register_alias),
        FunctionEntry::sync("stream_wrapper_unregister", &[PROTOCOL], unregister),
        FunctionEntry::sync("stream_wrapper_restore", &[PROTOCOL], restore),
        FunctionEntry::sync("stream_get_wrappers", &[], list),
    ] {
        registry.function(function);
    }
    for (name, number) in [
        ("STREAM_USE_PATH", USE_PATH),
        ("STREAM_IGNORE_URL", 2),
        ("STREAM_REPORT_ERRORS", REPORT_ERRORS),
        ("STREAM_MKDIR_RECURSIVE", 1),
        ("STREAM_URL_STAT_LINK", 1),
        ("STREAM_URL_STAT_QUIET", 2),
        ("STREAM_IS_URL", IS_URL),
        ("STREAM_CAST_AS_STREAM", 0),
        ("STREAM_CAST_FOR_SELECT", 3),
        ("STREAM_META_TOUCH", 1),
        ("STREAM_META_OWNER_NAME", 2),
        ("STREAM_META_OWNER", 3),
        ("STREAM_META_GROUP_NAME", 4),
        ("STREAM_META_GROUP", 5),
        ("STREAM_META_ACCESS", 6),
        ("STREAM_OPTION_BLOCKING", 1),
        ("STREAM_OPTION_READ_BUFFER", 2),
        ("STREAM_OPTION_WRITE_BUFFER", 3),
        ("STREAM_OPTION_READ_TIMEOUT", 4),
        ("STREAM_BUFFER_NONE", 0),
        ("STREAM_BUFFER_LINE", 1),
        ("STREAM_BUFFER_FULL", 2),
    ] {
        registry.constant(name, number);
    }
}

#[derive(Clone)]
pub(crate) struct UserWrapper {
    pub class: String,
    pub scheme: Vec<u8>,
    pub remote: bool,
}

#[derive(Clone)]
pub(crate) enum Handler {
    Builtin,
    User(UserWrapper),
}

/// PHP keeps the original insertion order until a scheme is removed and reinserted.
#[derive(Clone)]
pub(crate) struct Wrappers(Vec<(Vec<u8>, Handler)>);

impl Default for Wrappers {
    fn default() -> Self {
        Self(
            BUILTINS
                .iter()
                .map(|&name| (name.to_vec(), Handler::Builtin))
                .collect(),
        )
    }
}

impl Wrappers {
    fn intercepts_includes(&self) -> bool {
        self.0
            .iter()
            .any(|(_, handler)| matches!(handler, Handler::User(_)))
            || self.get(b"file").is_none()
    }

    fn find(&self, name: &[u8]) -> Option<usize> {
        self.0.iter().position(|(scheme, _)| scheme == name)
    }

    pub fn get(&self, name: &[u8]) -> Option<&Handler> {
        self.find(name).map(|index| &self.0[index].1)
    }

    fn remove(&mut self, name: &[u8]) -> bool {
        let Some(index) = self.find(name) else {
            return false;
        };
        self.0.remove(index);
        true
    }
}

fn register_function(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(add(context, arguments, "stream_wrapper_register"))
}

fn register_alias(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(add(context, arguments, "stream_register_wrapper"))
}

async fn add(context: &mut dyn Context, arguments: Vec<Value>, function: &str) -> NativeResult {
    let protocol = args::bytes(&arguments, 0);
    let class = String::from_utf8_lossy(args::bytes(&arguments, 1));
    let Some(class_handle) = context.lookup_class(&class).await? else {
        return Err(NativeError::type_error(format!(
            "{function}(): Argument #2 ($class) must be a valid class name, {class} given"
        )));
    };
    let files = files(context);
    files.claim_id();
    let wrappers = files.wrappers.get_or_insert_with(Wrappers::default);
    let scheme = String::from_utf8_lossy(protocol);
    if wrappers.get(protocol).is_some() {
        context.warning(format!(
            "{function}(): Protocol {scheme}:// is already defined."
        ));
        return Ok(Value::Bool(false));
    }
    if !protocol.iter().copied().all(scheme_byte) {
        let class = class_handle.name();
        context.warning(format!(
            "{function}(): Invalid protocol scheme specified. Unable to register wrapper class {class} to {scheme}://"
        ));
        return Ok(Value::Bool(false));
    }
    let remote = args::int(&arguments, 2).unwrap_or_default() & IS_URL != 0;
    wrappers.0.push((
        protocol.to_vec(),
        Handler::User(UserWrapper {
            class: class_handle.name().to_owned(),
            scheme: protocol.to_vec(),
            remote,
        }),
    ));
    context.set_include_wrappers(true);
    Ok(Value::Bool(true))
}

pub(crate) fn scheme_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"+-.".contains(&byte)
}

pub(crate) fn protocol(path: &[u8]) -> Option<&[u8]> {
    let end = path.iter().take_while(|&&byte| scheme_byte(byte)).count();
    (end > 1 && (path[end..].starts_with(b"://") || path.starts_with(b"data:")))
        .then_some(&path[..end])
}

/// The first lookup is case sensitive, then PHP retries with lowercase.
pub(crate) fn locate(context: &mut dyn Context, path: &[u8]) -> Option<UserWrapper> {
    let wrappers = context
        .state()
        .get::<crate::request::Files>()?
        .wrappers
        .as_ref()?;
    if matches!(
        crate::wrapper::Target::parse(path),
        crate::wrapper::Target::RemoteFile
    ) {
        return None;
    }
    let protocol = protocol(path).unwrap_or(b"file");
    let handler = wrappers
        .get(protocol)
        .or_else(|| wrappers.get(&protocol.to_ascii_lowercase()))
        .or_else(|| wrappers.get(b"file"))?;
    match handler {
        Handler::Builtin => None,
        Handler::User(wrapper) => Some(wrapper.clone()),
    }
}

pub(crate) fn disabled_scheme<'a>(context: &mut dyn Context, path: &'a [u8]) -> Option<&'a [u8]> {
    let wrappers = context
        .state()
        .get::<crate::request::Files>()?
        .wrappers
        .as_ref()?;
    let scheme = protocol(path)?;
    if scheme.eq_ignore_ascii_case(b"file") {
        return None;
    }
    BUILTINS
        .iter()
        .any(|builtin| builtin.eq_ignore_ascii_case(scheme))
        .then_some(scheme)
        .filter(|scheme| {
            wrappers.get(scheme).is_none() && wrappers.get(&scheme.to_ascii_lowercase()).is_none()
        })
}

pub(crate) fn protocol_disabled(context: &mut dyn Context, name: &[u8]) -> bool {
    context
        .state()
        .get::<crate::request::Files>()
        .and_then(|files| files.wrappers.as_ref())
        .is_some_and(|wrappers| {
            BUILTINS
                .iter()
                .any(|builtin| builtin.eq_ignore_ascii_case(name))
                && wrappers.get(name).is_none()
                && wrappers.get(&name.to_ascii_lowercase()).is_none()
        })
}

pub(crate) fn file_disabled(context: &mut dyn Context) -> bool {
    context
        .state()
        .get::<crate::request::Files>()
        .and_then(|files| files.wrappers.as_ref())
        .is_some_and(|wrappers| wrappers.get(b"file").is_none())
}

fn unregister(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let name = args::bytes(arguments, 0);
    let wrappers = files(context)
        .wrappers
        .get_or_insert_with(Wrappers::default);
    if wrappers.remove(name) {
        let intercepts = wrappers.intercepts_includes();
        context.set_include_wrappers(intercepts);
        return Ok(Value::Bool(true));
    }
    let scheme = String::from_utf8_lossy(name);
    context.warning(format!(
        "stream_wrapper_unregister(): Unable to unregister protocol {scheme}://"
    ));
    Ok(Value::Bool(false))
}

fn restore(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let name = args::bytes(arguments, 0);
    let scheme = String::from_utf8_lossy(name);
    if !BUILTINS.contains(&name) {
        context.warning(format!(
            "stream_wrapper_restore(): {scheme}:// never existed, nothing to restore"
        ));
        return Ok(Value::Bool(false));
    }
    let wrappers = files(context)
        .wrappers
        .get_or_insert_with(Wrappers::default);
    if matches!(wrappers.get(name), Some(Handler::Builtin)) {
        context.notice(format!(
            "stream_wrapper_restore(): {scheme}:// was never changed, nothing to restore"
        ));
    } else {
        wrappers.remove(name);
        wrappers.0.push((name.to_vec(), Handler::Builtin));
        let intercepts = wrappers.intercepts_includes();
        context.set_include_wrappers(intercepts);
    }
    Ok(Value::Bool(true))
}

fn list(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    let mut result = PhpArray::new();
    if let Some(wrappers) = &files(context).wrappers {
        for (name, _) in &wrappers.0 {
            result.push(Value::from(name.clone()))?;
        }
    } else {
        for &name in BUILTINS {
            result.push(Value::from(name.to_vec()))?;
        }
    }
    Ok(Value::from(result))
}

impl UserWrapper {
    pub fn permitted(&self, context: &mut dyn Context, function: &str) -> bool {
        if self.remote && !crate::user_file::enabled(context, b"allow_url_fopen", true) {
            let scheme = String::from_utf8_lossy(&self.scheme);
            context.warning(format!(
                "{function}(): {scheme}:// wrapper is disabled in the server configuration by allow_url_fopen=0"
            ));
            return false;
        }
        true
    }
}
