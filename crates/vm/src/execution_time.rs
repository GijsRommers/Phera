//! `set_time_limit(1); while (true) {}` aborts after a second of PHP CPU time.
//! The request's clock runs only while its future is being polled. The
//! interpreter samples that clock at the existing scheduler handoffs.

use std::cell::Cell;
use std::future::{Future, poll_fn};
use std::rc::Rc;
use std::time::Duration;

use phera_extension_api::NativeError;
use rustix::time::{ClockId, clock_gettime};

use crate::Vm;

#[derive(Default)]
pub(crate) struct ExecutionTime {
    seconds: Cell<i64>,
    charged: Cell<Duration>,
    process_waits: Cell<Duration>,
    started: Cell<Option<Duration>>,
    polling: Cell<bool>,
    timed_out: Cell<bool>,
}

impl ExecutionTime {
    pub(crate) fn restart(&self, seconds: i64) {
        self.seconds.set(seconds.max(0));
        self.charged.set(Duration::ZERO);
        self.process_waits.set(Duration::ZERO);
        self.started
            .set((seconds > 0 && self.polling.get()).then(thread_cpu_time));
    }

    fn elapsed(&self) -> Duration {
        self.charged.get().saturating_add(self.process_waits.get())
    }

    fn charge(&self) {
        if let Some(started) = self.started.get() {
            let now = thread_cpu_time();
            self.charged.set(
                self.charged
                    .get()
                    .saturating_add(now.saturating_sub(started)),
            );
            self.started.set(Some(now));
        }
    }

    pub(crate) async fn track<T>(self: Rc<Self>, future: impl Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        poll_fn(|context| {
            self.polling.set(true);
            self.started
                .set((self.seconds.get() > 0).then(thread_cpu_time));
            let result = future.as_mut().poll(context);
            if result.is_pending() {
                self.charge();
            }
            self.started.set(None);
            self.polling.set(false);
            result
        })
        .await
    }
}

impl Vm {
    #[cold]
    pub(crate) fn check_execution_time(&mut self) -> Result<(), NativeError> {
        let seconds = self.execution_time.seconds.get();
        if seconds == 0 {
            return Ok(());
        }
        self.execution_time.charge();
        if self.execution_time.elapsed().as_secs() < seconds as u64 {
            return Ok(());
        }
        Err(self.execution_time_error(seconds))
    }

    pub(crate) fn process_wait_budget(&self) -> Option<Duration> {
        let seconds = self.execution_time.seconds.get();
        if seconds == 0 {
            return None;
        }
        self.execution_time.charge();
        Some(Duration::from_secs(seconds as u64).saturating_sub(self.execution_time.elapsed()))
    }

    pub(crate) fn process_wait_elapsed(&self, elapsed: Duration) {
        let waits = &self.execution_time.process_waits;
        waits.set(waits.get().saturating_add(elapsed));
    }

    pub(crate) fn process_wait_expired(&mut self) -> NativeError {
        self.execution_time_error(self.execution_time.seconds.get())
    }

    fn execution_time_error(&mut self, seconds: i64) -> NativeError {
        self.execution_time.restart(seconds);
        if let Some(cancel) = self
            .state_mut()
            .get::<phera_extension_api::ProcessCancellation>()
        {
            cancel.cancel();
        }
        let mut location = self.location();
        // The fairness boundary publishes the target of a taken branch.
        // Zend reports the opcode where execution would resume.
        if self.diagnostic_location.is_none()
            && let Some(frame) = self.current()
            && let Some(&line) = frame.function().lines.get(frame.ip())
        {
            location.line = line as usize;
        }
        let suffix = if seconds == 1 { "" } else { "s" };
        self.execution_time.timed_out.set(true);
        self.set_hidden_fatal_status();
        self.fatal_error(
            1,
            format!("Maximum execution time of {seconds} second{suffix} exceeded"),
            location,
        )
    }

    /// Timeouts decide their HTTP response before entering shutdown.
    pub fn timed_out(&self) -> bool {
        self.execution_time.timed_out.get()
    }
}

fn thread_cpu_time() -> Duration {
    let time = clock_gettime(ClockId::ThreadCPUTime);
    Duration::new(time.tv_sec as u64, time.tv_nsec as u32)
}
