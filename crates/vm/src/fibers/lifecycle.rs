//! Destroying a suspended fiber unwinds it: its finally blocks run and its
//! variables are released, frame by frame, as a graceful close.
//!
//! ```php
//! $fiber = new Fiber(function () {
//!     $lock = new Lock;
//!     try { Fiber::suspend(); } finally { echo "finally\n"; }
//! });
//! $fiber->start();
//! unset($fiber);   // "finally", then $lock's destructor
//! ```
//!
//! PHP does this in the fiber object's destructor, so it happens when the
//! last reference goes, or in the destructor phases at the end of the
//! request, in their order; never after a fatal error. Phera gives a
//! suspended fiber a due destructor (`Object::expect_destructor`) and holds
//! it only weakly, so the interpreter's destructor dispatch reaches it at
//! the same moments and calls [`Vm::close_fiber`].

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

    /// The fiber suspended: it is destroyed (unwound) when it goes.
    pub(super) fn park_suspended(&mut self, object: Rc<Object>) {
        object.expect_destructor();
        self.suspended_fibers
            .insert(object.id, Rc::downgrade(&object));
    }

    /// Whether `object` is a fiber that is suspended.
    pub(crate) fn is_suspended_fiber(object: &Object) -> bool {
        with_state(object, |fiber| matches!(fiber.state, State::Suspended(_))).unwrap_or(false)
    }

    /// Destroys a suspended fiber: unwinds it gracefully. An exception its
    /// finally blocks throw goes on from where the fiber was released.
    pub(crate) async fn close_fiber(&mut self, object: Rc<Object>) -> NativeResult {
        self.suspended_fibers.remove(&object.id);
        if self.fiber_bailout {
            return Ok(phera_value::Value::Null);
        }
        let mut suspended = take_suspended(&object)?;
        suspended.exchange.site.replace(None);
        let progress = self.poll_fiber(&mut suspended, Input::Close).await?;
        self.finish_fiber_progress(object, suspended, progress)
    }
}
