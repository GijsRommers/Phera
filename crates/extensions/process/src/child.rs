//! `proc_close($child)` waits and reaps. On the web, an abandoned child is killed.

use std::io;
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;

use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use phera_extension_api::ProcessCancellation;
use tokio::process::Child;

pub(crate) struct ManagedChild {
    pub child: Child,
    group: bool,
    kill_when_dropped: bool,
    cancelled: ProcessCancellation,
}

impl ManagedChild {
    pub fn new(child: Child, group: bool, cancelled: ProcessCancellation) -> Self {
        Self {
            child,
            group,
            kill_when_dropped: true,
            cancelled,
        }
    }

    pub fn detach_on_cli_cleanup(&mut self) {
        self.kill_when_dropped = self.group;
    }

    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        // Cancelling a wait must release the child as well as its pipes.
        self.kill_when_dropped = true;
        let status = self.child.wait().await?;
        self.kill_when_dropped = false;
        Ok(status)
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        if !self.kill_when_dropped && !self.cancelled.is_cancelled() {
            return;
        }
        if self.group
            && let Some(id) = self.child.id()
        {
            let _ = killpg(Pid::from_raw(id as i32), Signal::SIGKILL);
        }
        let _ = self.child.start_kill();
    }
}

pub(crate) fn exit_code(status: ExitStatus) -> i64 {
    i64::from(status.code().unwrap_or_else(|| status.into_raw()))
}

pub(crate) fn send_signal(pid: i32, signal: i32) -> Result<(), nix::errno::Errno> {
    let signal = if signal == 0 {
        None
    } else {
        Some(Signal::try_from(signal)?)
    };
    nix::sys::signal::kill(Pid::from_raw(pid), signal)
}
