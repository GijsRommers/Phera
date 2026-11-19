//! Unix process control and POSIX queries.
//!
//! ```php
//! $child = proc_open(['printf', '%s', 'hello'], [1 => ['pipe', 'w']], $pipes);
//! echo stream_get_contents($pipes[1]);
//! proc_close($child);
//! ```

mod child;
mod command;
mod descriptors;
mod pcntl;
mod posix;
mod proc;
mod shell;

use phera_extension_api::{Context, Extension, NativeError, Registry, RequestState};

pub struct Process;
pub use pcntl::register_cli as register_cli_process_control;
pub use posix::Posix;

impl Extension for Process {
    fn name(&self) -> &'static str {
        "process"
    }

    fn php_module(&self) -> Option<&'static str> {
        Some("standard")
    }

    fn register(&self, registry: &mut Registry) {
        proc::register(registry);
        shell::register(registry);
        shell::popen::register(registry);
    }
}

#[derive(Default)]
struct Policy {
    web: bool,
    disabled: bool,
}

pub fn configure_web(state: &mut RequestState, disabled: bool) {
    *state.get_or_default::<Policy>() = Policy {
        web: true,
        disabled,
    };
}

fn permitted(context: &mut dyn Context, function: &str) -> Result<bool, NativeError> {
    let policy = context.state().get_or_default::<Policy>();
    if policy.disabled {
        return Err(NativeError::error(format!(
            "{function}(): Starting programs is disabled by phera serve --no-process"
        )));
    }
    Ok(policy.web)
}
