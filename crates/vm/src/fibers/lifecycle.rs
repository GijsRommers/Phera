//! After shutdown callbacks, a fiber parked in `try { Fiber::suspend(); }`
//! receives a graceful unwinding signal and runs its finally block.

use std::rc::Rc;

use phera_extension_api::{NativeResult, fiber::FiberInspection};
use phera_value::Object;

use crate::Vm;

use super::{State, stack::Input, switch::take_suspended, trace::TraceEntry, with_state};

impl Vm {
    pub(crate) async fn fiber_inspect(
        &mut self,
        object: Rc<Object>,
        inspection: FiberInspection,
    ) -> NativeResult {
        if self
            .current_fiber()
            .is_some_and(|current| Rc::ptr_eq(&object, &current))
        {
            return self.fiber_snapshot(inspection, TraceEntry::Reflection);
        }
        let running = with_state(&object, |fiber| {
            matches!(fiber.state, State::Running)
                .then(|| fiber.live.as_ref()?.upgrade())
                .flatten()
        })?;
        if let Some(exchange) = running {
            let waiting = exchange.waiting.borrow();
            return waiting
                .as_ref()
                .ok_or_else(super::reflection::invalid_state)?
                .inspect(inspection);
        }
        let mut suspended =
            take_suspended(&object).map_err(|_| super::reflection::invalid_state())?;
        let progress = self
            .poll_fiber(&mut suspended, Input::Inspect(inspection))
            .await?;
        self.finish_fiber_progress(object, suspended, progress)
    }

    pub(crate) async fn close_fibers(&mut self) {
        if self.fiber_bailout {
            self.suspended_fibers.clear();
            return;
        }
        let mut failure = None;
        while let Some((_, object)) = self.suspended_fibers.pop_first() {
            let Ok(mut suspended) = take_suspended(&object) else {
                continue;
            };
            suspended.exchange.site.replace(None);
            let result = match self.poll_fiber(&mut suspended, Input::Close).await {
                Ok(progress) => self.finish_fiber_progress(object, suspended, progress),
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                failure = Some(match failure {
                    Some(previous) => self.exception_with_previous(error, previous),
                    None => error,
                });
            }
        }
        if let Some(error) = failure {
            self.report_uncaught(error).await;
        }
    }
}
