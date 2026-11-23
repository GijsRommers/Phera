//! `fopen('virtual://report', 'r')` calls a new wrapper object for each handle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, NativeError, PhpArray, Value,
    value::{Object, Resource, to_bool},
};

use crate::errors::open_failure_message;
use crate::io_policy::CHUNK;
use crate::user_wrappers::UserWrapper;
use crate::wrapper::Target;

mod operations;
mod options;

pub(crate) const OPEN_FOR_INCLUDE: i64 = 128;

pub(crate) struct UserFile {
    pub object: Rc<Object>,
    pub context: Value,
    pub opened_path: Option<Vec<u8>>,
    pub uri: Vec<u8>,
    pub mode: Vec<u8>,
    pub remote: bool,
    pub position: Cell<i64>,
    pub seekable: Cell<bool>,
    pub chunk: Cell<usize>,
    pub eof: Cell<bool>,
    pub buffer: RefCell<std::collections::VecDeque<u8>>,
    writing: Cell<bool>,
    busy: Cell<bool>,
}

pub(crate) fn resource_file(resource: &Resource) -> Option<Rc<UserFile>> {
    resource
        .data
        .borrow()
        .as_ref()?
        .downcast_ref::<crate::user_resource::UserHandle<UserFile>>()
        .map(|file| file.value.clone())
}

impl UserWrapper {
    pub(crate) async fn object(
        &self,
        context: &mut dyn Context,
        stream_context: Value,
    ) -> Result<Rc<Object>, NativeError> {
        let mut properties = PhpArray::new();
        properties.insert(ArrayKey::str("context"), stream_context);
        let Value::Object(object) = context.create_object(&self.class, properties)? else {
            return Err(NativeError::error(
                "Stream wrapper allocation did not return an object",
            ));
        };
        invoke(context, &object, "__construct", Vec::new()).await?;
        Ok(object)
    }
}

pub(crate) async fn invoke(
    context: &mut dyn Context,
    object: &Rc<Object>,
    method: &str,
    arguments: Vec<Value>,
) -> Result<Option<Value>, NativeError> {
    if context.has_method(object, method) {
        context
            .call_serialization_method(object.clone(), method, arguments)
            .await
            .map(Some)
    } else {
        Ok(None)
    }
}

pub(crate) fn missing(
    context: &mut dyn Context,
    object: &Object,
    function: &str,
    method: &str,
    suffix: &str,
) {
    context.warning(format!(
        "{function}(): {}::{method} is not implemented!{suffix}",
        object.class.name()
    ));
}

pub(crate) async fn open(
    context: &mut dyn Context,
    wrapper: UserWrapper,
    function: &str,
    uri: &[u8],
    mode: &[u8],
    flags: i64,
    stream_context: Value,
) -> Result<Option<Rc<UserFile>>, NativeError> {
    let path = match Target::parse(uri) {
        Target::File(path) => path,
        _ => uri,
    };
    if wrapper.remote {
        let directive = if !enabled(context, b"allow_url_fopen", true) {
            Some("allow_url_fopen")
        } else if (flags & OPEN_FOR_INCLUDE != 0 || restricted_include(context))
            && !enabled(context, b"allow_url_include", false)
        {
            Some("allow_url_include")
        } else {
            None
        };
        if let Some(directive) = directive {
            let scheme = crate::user_wrappers::protocol(uri).unwrap_or(b"file");
            context.warning(format!(
                "{function}(): {}:// wrapper is disabled in the server configuration by {directive}=0",
                String::from_utf8_lossy(scheme)
            ));
            failed_open(context, function, uri, "no suitable wrapper could be found");
            return Ok(None);
        }
    }
    let opens = context.state().get_or_default::<OpenWrappers>().0.clone();
    let restricted =
        flags & OPEN_FOR_INCLUDE != 0 && !enabled(context, b"allow_url_include", false);
    if opens.borrow().iter().any(|active| active.0 == path) {
        failed_open(context, function, uri, "infinite recursion prevented");
        return Ok(None);
    }
    let key = Rc::new((path.to_vec(), restricted));
    opens.borrow_mut().push(key.clone());
    let _opening = Opening { opens, key };

    let object = wrapper.object(context, stream_context.clone()).await?;
    let opened = Rc::new(RefCell::new(Value::Null));
    let result = invoke(
        context,
        &object,
        "stream_open",
        vec![
            Value::from(path.to_vec()),
            Value::from(mode.to_vec()),
            Value::Int(flags & !crate::user_wrappers::REPORT_ERRORS),
            Value::Reference(opened.clone()),
        ],
    )
    .await?;
    let reason = match result {
        Some(value) if to_bool(&value) => None,
        None => Some("is not implemented"),
        _ => Some("call failed"),
    };
    if let Some(reason) = reason {
        failed_open(
            context,
            function,
            uri,
            &format!("\"{}::stream_open\" {reason}", wrapper.class),
        );
        return Ok(None);
    }
    let opened_path = opened.borrow().as_str_bytes().map(|path| path.to_vec());
    let file = Rc::new(UserFile {
        object,
        context: stream_context,
        opened_path,
        uri: uri.to_vec(),
        mode: mode[..mode.len().min(15)].to_vec(),
        remote: wrapper.remote,
        position: Cell::new(0),
        seekable: Cell::new(true),
        chunk: Cell::new(CHUNK),
        eof: Cell::new(false),
        buffer: RefCell::new(std::collections::VecDeque::new()),
        writing: Cell::new(false),
        busy: Cell::new(false),
    });
    if mode.first() == Some(&b'a') {
        file.seek_callback(context, function, 0, 1).await?;
    }
    Ok(Some(file))
}

pub(crate) fn enabled(context: &dyn Context, directive: &[u8], default: bool) -> bool {
    context
        .ini_get(directive)
        .map_or(default, |value| !matches!(value.as_bytes(), b"" | b"0"))
}

fn failed_open(context: &mut dyn Context, function: &str, uri: &[u8], reason: &str) {
    context.warning_bytes(&open_failure_message(function, uri, reason.as_bytes()));
}

type OpenList = Rc<RefCell<Vec<Rc<(Vec<u8>, bool)>>>>;

#[derive(Default)]
struct OpenWrappers(OpenList);

pub(crate) fn register_fork(registry: &mut phera_extension_api::Registry) {
    registry.fork_state::<OpenWrappers>(phera_extension_api::fresh_state);
}

fn restricted_include(context: &mut dyn Context) -> bool {
    context
        .state()
        .get::<OpenWrappers>()
        .is_some_and(|opens| opens.0.borrow().iter().any(|active| active.1))
}

struct Opening {
    opens: OpenList,
    key: Rc<(Vec<u8>, bool)>,
}

impl Drop for Opening {
    fn drop(&mut self) {
        self.opens
            .borrow_mut()
            .retain(|key| !Rc::ptr_eq(key, &self.key));
    }
}

impl UserFile {
    async fn invoke(
        &self,
        context: &mut dyn Context,
        method: &str,
        arguments: Vec<Value>,
    ) -> Result<Option<Value>, NativeError> {
        if self.busy.replace(true) {
            return Err(NativeError::error(
                "Stream wrapper is already executing a callback",
            ));
        }
        let _busy = Busy(&self.busy);
        invoke(context, &self.object, method, arguments).await
    }

    pub(crate) fn busy(&self) -> bool {
        self.busy.get()
    }
}

struct Busy<'a>(&'a Cell<bool>);

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
