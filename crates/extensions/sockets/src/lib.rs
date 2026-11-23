//! `socket_create(AF_INET, SOCK_STREAM, SOL_TCP)` opens a worker-owned socket.

mod address;
mod classes;
mod constants;
mod data;
mod lifecycle;
mod messages;
mod options;
mod status;

use std::io;

use phera_extension_api::{Context, Extension, NativeResult, Param, ParamType, Registry, Value};

pub struct Sockets;

pub(crate) const SOCKET: Param = Param::new("socket", ParamType::Class("Socket"));
pub(crate) const ADDRESS: Param = Param::new("address", ParamType::String);
pub(crate) const LENGTH: Param = Param::new("length", ParamType::Int);
pub(crate) const FLAGS: Param = Param::new("flags", ParamType::Int);
pub(crate) const PORT: Param = Param::new("port", ParamType::Int);

impl Extension for Sockets {
    fn name(&self) -> &'static str {
        "sockets"
    }

    fn register(&self, registry: &mut Registry) {
        constants::register(registry);
        classes::register(registry);
        lifecycle::register(registry);
        address::register(registry);
        messages::register(registry);
        data::register(registry);
        options::register(registry);
        status::register(registry);
    }
}

fn failure(
    context: &mut dyn Context,
    handle: Option<&classes::Handle>,
    function: &str,
    action: &str,
    error: &io::Error,
) -> NativeResult {
    let code = error.raw_os_error().unwrap_or(nix::libc::EINVAL);
    status::set_error(context, handle, code);
    context.warning(format!(
        "{function}(): {action} [{code}]: {}",
        status::error_text(code)
    ));
    Ok(Value::Bool(false))
}
