//! Socket and AddressInfo are opaque objects created by their factory functions.

use std::{
    any::Any,
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use phera_ext_streams::StreamSocket;
use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, Context, NativeClone, NativeError, NativeResult, Object,
    PhpArray, Registry, Value, args, value::Fork,
};

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        hooks: Some(&ClassHooks {
            construction_error: Some(
                "Cannot directly construct Socket, use socket_create() instead",
            ),
            clone: Some(NativeClone::Copy(reject_clone)),
            fork: Some(fork_socket),
            forbid_serialization: true,
            ..ClassHooks::EMPTY
        }),
        ..ClassEntry::class("Socket")
    });
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        hooks: Some(&ClassHooks {
            construction_error: Some(
                "Cannot directly construct AddressInfo, use socket_addrinfo_lookup() instead",
            ),
            clone: Some(NativeClone::Copy(reject_clone)),
            fork: Some(crate::address::info::fork),
            forbid_serialization: true,
            ..ClassHooks::EMPTY
        }),
        ..ClassEntry::class("AddressInfo")
    });
}

fn reject_clone(original: &Rc<Object>, _: &Object) -> Result<(), NativeError> {
    Err(NativeError::error(format!(
        "Trying to clone an uncloneable object of class {}",
        original.class.name()
    )))
}

fn fork_socket(_: &Object, state: &dyn Any, _: &mut Fork<'_>) -> Result<Box<dyn Any>, String> {
    let handle = state
        .downcast_ref::<Rc<Handle>>()
        .ok_or("unknown Socket state")?;
    if handle.is_open() {
        return Err("an open Socket belongs to the boot".into());
    }
    Ok(Box::new(Rc::new(Handle {
        socket: RefCell::new(None),
        stream: RefCell::new(Value::Null),
        blocking: Cell::new(true),
        error: Cell::new(handle.error.get()),
    })))
}

pub(crate) fn create<T: 'static>(context: &mut dyn Context, name: &str, state: T) -> NativeResult {
    let value = context.create_object(name, PhpArray::new())?;
    let Value::Object(object) = &value else {
        return Err(NativeError::error(
            "Socket factory did not return an object",
        ));
    };
    *object.native.borrow_mut() = Some(Box::new(state));
    Ok(value)
}

pub(crate) fn socket(
    context: &mut dyn Context,
    socket: Arc<StreamSocket>,
    stream: Value,
) -> NativeResult {
    let socket = stream.is_null().then_some(socket);
    create(
        context,
        "Socket",
        Rc::new(Handle {
            socket: RefCell::new(socket),
            stream: RefCell::new(stream),
            blocking: Cell::new(true),
            error: Cell::new(0),
        }),
    )
}

pub(crate) fn argument(arguments: &[Value], name: &str) -> Result<Rc<Handle>, NativeError> {
    let handle = args::object(arguments, 0)
        .and_then(|object| {
            object
                .native
                .borrow()
                .as_ref()?
                .downcast_ref::<Rc<Handle>>()
                .cloned()
        })
        .filter(|handle| handle.is_open());
    handle.ok_or_else(|| closed(name))
}

pub(crate) fn closed(function: &str) -> NativeError {
    NativeError::error(format!(
        "{function}(): Argument #1 ($socket) has already been closed"
    ))
}

pub(crate) struct Handle {
    socket: RefCell<Option<Arc<StreamSocket>>>,
    stream: RefCell<Value>,
    blocking: Cell<bool>,
    pub(crate) error: Cell<i32>,
}

impl Handle {
    fn is_open(&self) -> bool {
        match &*self.stream.borrow() {
            Value::Resource(resource) => !resource.is_closed(),
            _ => self.socket.borrow().is_some(),
        }
    }

    pub(crate) fn raw(&self, function: &str) -> Result<Arc<StreamSocket>, NativeError> {
        phera_ext_streams::import_socket_stream(&self.stream.borrow())
            .or_else(|| self.socket.borrow().clone())
            .ok_or_else(|| closed(function))
    }

    pub(crate) fn is_blocking(&self) -> bool {
        phera_ext_streams::socket_stream_blocking(&self.stream.borrow())
            .unwrap_or_else(|| self.blocking.get())
    }

    pub(crate) fn set_blocking(&self, blocking: bool) {
        phera_ext_streams::set_socket_stream_blocking(&self.stream.borrow(), blocking);
        self.blocking.set(blocking);
    }

    pub(crate) fn export(&self, context: &mut dyn Context) -> NativeResult {
        let mut stream = self.stream.borrow_mut();
        if stream.is_null() {
            let socket = self
                .socket
                .borrow_mut()
                .take()
                .ok_or_else(|| closed("socket_export_stream"))?;
            *stream = phera_ext_streams::export_socket_stream(context, socket, self.blocking.get());
        }
        Ok(stream.clone())
    }

    pub(crate) async fn close(&self, context: &mut dyn Context) -> NativeResult {
        let stream = self.stream.replace(Value::Null);
        self.socket.borrow_mut().take();
        phera_ext_streams::close_socket_stream(context, stream).await
    }
}
