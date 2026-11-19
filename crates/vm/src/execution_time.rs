//! `set_time_limit(1); while (true) {}` aborts after a second of PHP CPU time.
//! The request's clock runs only while its future is being polled. The
//! interpreter samples that clock at the existing scheduler handoffs.

use std::cell::Cell;
use std::future::{Future, poll_fn};
use std::rc::Rc;
use std::time::Duration;

use phera_extension_api::{NativeError, Unwind};
use rustix::time::{ClockId, clock_gettime};

use crate::Vm;

#[derive(Default)]
pub(crate) struct ExecutionTime {
    seconds: Cell<i64>,
    charged: Cell<Duration>,
    started: Cell<Option<Duration>>,
    polling: Cell<bool>,
    timeout_http_failure: Cell<Option<bool>>,
}

impl ExecutionTime {
    pub(crate) fn restart(&self, seconds: i64) {
        self.seconds.set(seconds.max(0));
        self.charged.set(Duration::ZERO);
        self.started
            .set((seconds > 0 && self.polling.get()).then(thread_cpu_time));
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
            self.charge();
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
        if self.execution_time.charged.get().as_secs() < seconds as u64 {
            return Ok(());
        }
        self.execution_time.restart(seconds);
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
        self.execution_time.timeout_http_failure.set(Some(
            !self.error_handlers.display_errors && self.output_writer.origin.is_none(),
        ));
        self.mark_timed_out_objects();
        self.report_fatal(
            1,
            format!("Maximum execution time of {seconds} second{suffix} exceeded"),
            location,
        );
        Err(NativeError::Unwind(Unwind(Box::new(()))))
    }

    /// A hidden fatal timeout can set HTTP 500 only before the response is committed.
    pub fn timeout_http_failure(&self) -> Option<bool> {
        self.execution_time.timeout_http_failure.get()
    }
}

fn thread_cpu_time() -> Duration {
    let time = clock_gettime(ClockId::ThreadCPUTime);
    Duration::new(time.tv_sec as u64, time.tv_nsec as u32)
}
