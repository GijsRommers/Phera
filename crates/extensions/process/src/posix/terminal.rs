//! `posix_isatty(0)` tests stdin. `posix_ttyname(STDOUT)` asks for its terminal path.

use std::io::{stderr, stdin, stdout};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::ffi::OsStrExt;

use nix::{errno::Errno, unistd};
use phera_ext_streams::{stream_kind, with_descriptor};
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
    prepare_call,
};

const DESCRIPTOR: Param = Param::new("file_descriptor", ParamType::Mixed);
const NUMBER: Param = Param::new("file_descriptor", ParamType::Int);

pub(super) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("posix_isatty", &[DESCRIPTOR], isatty));
    registry.function(FunctionEntry::sync("posix_ttyname", &[DESCRIPTOR], ttyname));
}

fn isatty(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    terminal(context, arguments, "posix_isatty", false, |fd| {
        unistd::isatty(fd).and_then(|yes| {
            if yes {
                Ok(Value::Bool(true))
            } else {
                Err(Errno::ENOTTY)
            }
        })
    })
}

fn ttyname(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    terminal(context, arguments, "posix_ttyname", true, |fd| {
        unistd::ttyname(fd).map(|name| Value::from(name.as_os_str().as_bytes()))
    })
}

fn terminal(
    context: &mut dyn Context,
    arguments: &[Value],
    function: &str,
    coerce_on_error: bool,
    operation: impl FnOnce(BorrowedFd<'_>) -> Result<Value, Errno>,
) -> NativeResult {
    let original = args::value(arguments, 0);
    let result = if let Value::Resource(resource) = original {
        let Some(result) = with_descriptor(resource, operation) else {
            let reason = if let Some(kind) = stream_kind(resource) {
                format!("Could not use stream of type '{kind}'")
            } else {
                return Err(NativeError::type_error(format!(
                    "{function}(): supplied resource is not a valid stream resource"
                )));
            };
            context.warning(format!("{function}(): {reason}"));
            return Ok(Value::Bool(false));
        };
        result
    } else {
        let mut values = [original.clone()];
        let number = if prepare_call(function, &[NUMBER], &mut values, context).is_ok() {
            args::int(&values, 0).unwrap_or_default()
        } else {
            let given = crate::descriptors::given(original);
            context.warning(format!(
                "{function}(): Argument #1 ($file_descriptor) must be of type \
                 int|resource, {given} given",
            ));
            if !coerce_on_error {
                return Ok(Value::Bool(false));
            }
            phera_extension_api::value::to_int(original, context.diagnostics())?
        };
        match number {
            0 => operation(stdin().as_fd()),
            1 => operation(stdout().as_fd()),
            2 => operation(stderr().as_fd()),
            3..=2_147_483_647 => Err(Errno::EBADF),
            _ => {
                context.warning(format!(
                    "{function}(): Argument #1 ($file_descriptor) must be between 0 and {}",
                    i32::MAX,
                ));
                Err(Errno::EBADF)
            }
        }
    };
    Ok(result.unwrap_or_else(|error| super::fail(context, error)))
}
