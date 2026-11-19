//! `Fiber::suspend('ready')` parks an owned interpreter future. It can
//! receive a value, an exception, or a graceful close at the same call site.

use std::future::poll_fn;
use std::rc::Rc;
use std::task::Poll;

use phera_extension_api::fiber::{FiberOperation, fiber_error};
use phera_extension_api::{NativeError, NativeResult, Unwind};
use phera_value::{Object, Value};

use crate::{Vm, exceptions::Thrown};

use super::{
    Fiber, State,
    caller::CallerStack,
    stack::{Active, Exchange, Input, Suspended},
    trace::{Method, Site, TraceEntry, Waiting},
    with_state,
};

pub(crate) struct GracefulClose;

impl GracefulClose {
    pub(crate) fn error() -> NativeError {
        NativeError::Unwind(Unwind(Box::new(Self)))
    }
}

pub(super) enum Progress {
    Yielded(Value),
    Inspected(NativeResult),
    Finished(NativeResult),
}

impl Vm {
    pub(crate) async fn fiber_operation(
        &mut self,
        object: Rc<Object>,
        operation: FiberOperation,
    ) -> NativeResult {
        if let FiberOperation::Construct(callable) = operation {
            let callee = self
                .request_callable("Fiber::__construct", &callable)
                .await
                .map_err(|error| {
                    self.fiber_call_error(
                        "Fiber::__construct",
                        std::slice::from_ref(&callable),
                        error,
                    )
                })?;
            let mut state = object
                .native
                .try_borrow_mut()
                .map_err(|_| internal_error!("fiber constructor state borrowed"))?;
            if state.is_some() {
                drop(state);
                return Err(self.fiber_call_error(
                    "Fiber::__construct",
                    std::slice::from_ref(&callable),
                    fiber_error("Cannot call constructor twice"),
                ));
            }
            *state = Some(Box::new(Fiber {
                callable,
                live: None,
                state: State::Ready(callee),
            }));
            return Ok(Value::Null);
        }
        if self
            .active_fiber
            .as_ref()
            .is_some_and(|active| active.exchange.depth.get() >= 16)
        {
            return Err(fiber_error(
                "Cannot switch fibers in current execution context",
            ));
        }
        let (mut suspended, input, method) = match operation {
            FiberOperation::Start(arguments) => {
                self.check_fiber_stack_size()?;
                let callee = with_state(&object, |fiber| match fiber.state {
                    State::Ready(_) => match std::mem::replace(&mut fiber.state, State::Running) {
                        State::Ready(callee) => Ok(callee),
                        _ => Err(internal_error!("fiber start state")),
                    },
                    _ => Err(fiber_error(
                        "Cannot start a fiber that has already been started",
                    )),
                })
                .and_then(|result| result)
                .map_err(|error| {
                    let arguments = arguments
                        .iter()
                        .map(|(_, value)| value.clone())
                        .collect::<Vec<_>>();
                    self.fiber_call_error("Fiber::start", &arguments, error)
                })?;
                let site = Site::from_caller(self, Method::Start, &arguments);
                let exchange = Exchange::new(self);
                let mut child = Box::new(Vm::new_shell(self.registry.clone()));
                child.active_fiber = Some(Box::new(Active {
                    object: Rc::downgrade(&object),
                    exchange: exchange.clone(),
                    closing: false,
                }));
                with_state(&object, |fiber| fiber.live = Some(Rc::downgrade(&exchange)))?;
                let channel = exchange.clone();
                let future = Box::pin(async move {
                    channel.restore_child(&mut child)?;
                    channel.input.take();
                    let result = child.invoke_array_callback(callee, &arguments).await;
                    channel.park_child(&mut child)?;
                    result
                });
                (Suspended { exchange, future }, Input::Start, Some(site))
            }
            FiberOperation::Resume(value) => {
                let site = Site::with_value(self, Method::Resume, value.as_ref())?;
                let stack = self.take_resumable(&object, "Fiber::resume", value.as_slice())?;
                (stack, Input::Value(value.unwrap_or_default()), Some(site))
            }
            FiberOperation::Throw(value) => {
                let site = Site::with_value(self, Method::Throw, Some(&value))?;
                let stack =
                    self.take_resumable(&object, "Fiber::throw", std::slice::from_ref(&value))?;
                (stack, Input::Throw(value), Some(site))
            }
            FiberOperation::Construct(_) => {
                return Err(internal_error!("fiber constructor routing"));
            }
        };
        let waiting = self
            .active_fiber
            .as_ref()
            .map(|active| active.exchange.clone());
        if let (Some(waiting), Some(site)) = (&waiting, &method) {
            let entry = site.trace_entry(Some(Value::Object(object.clone())), false);
            waiting.waiting.replace(Some(Waiting::capture(self, entry)));
        }
        suspended.exchange.site.replace(method);
        let progress = self.poll_fiber(&mut suspended, input).await;
        if let Some(waiting) = waiting {
            waiting.waiting.replace(None);
        }
        let progress = progress?;
        self.finish_fiber_progress(object, suspended, progress)
    }

    pub(super) async fn poll_fiber(
        &mut self,
        suspended: &mut Suspended,
        input: Input,
    ) -> Result<Progress, NativeError> {
        self.export_main_globals();
        let exchange = &suspended.exchange;
        let depth = self
            .active_fiber
            .as_ref()
            .map_or(1, |active| active.exchange.depth.get() + 1);
        exchange.depth.set(depth);
        exchange.input.set(Some(input));
        exchange.transfer(self)?;
        if exchange
            .site
            .borrow()
            .as_ref()
            .is_some_and(|site| site.park_caller)
        {
            exchange.caller.replace(Some(CallerStack::take(self)));
        }
        let progress = poll_fn(|context| match suspended.future.as_mut().poll(context) {
            Poll::Ready(result) => Poll::Ready(Progress::Finished(result)),
            Poll::Pending => {
                if let Some(value) = exchange.yielded.take() {
                    Poll::Ready(Progress::Yielded(value))
                } else if let Some(result) = exchange.inspected.take() {
                    Poll::Ready(Progress::Inspected(result))
                } else {
                    Poll::Pending
                }
            }
        })
        .await;
        if let Some(caller) = exchange.caller.replace(None) {
            caller.restore(self);
        }
        exchange.transfer(self)?;
        self.import_main_globals();
        Ok(progress)
    }

    pub(super) fn finish_fiber_progress(
        &mut self,
        object: Rc<Object>,
        suspended: Suspended,
        progress: Progress,
    ) -> NativeResult {
        match progress {
            Progress::Yielded(value) => {
                with_state(&object, |fiber| fiber.state = State::Suspended(suspended))?;
                self.suspended_fibers.entry(object.id).or_insert(object);
                Ok(value)
            }
            Progress::Inspected(result) => {
                with_state(&object, |fiber| fiber.state = State::Suspended(suspended))?;
                self.suspended_fibers.entry(object.id).or_insert(object);
                result
            }
            Progress::Finished(result) => {
                self.suspended_fibers.remove(&object.id);
                let state = match &result {
                    Ok(value) => State::Returned(value.clone()),
                    Err(NativeError::Unwind(payload))
                        if payload.0.is::<()>()
                            || payload.0.is::<crate::error_traces::DeclarationFailure>() =>
                    {
                        self.fiber_bailout = self.status == 255
                            || payload.0.is::<crate::error_traces::DeclarationFailure>();
                        State::Exited
                    }
                    Err(_) => State::Failed,
                };
                with_state(&object, |fiber| {
                    fiber.state = state;
                    fiber.callable = Value::Null;
                    fiber.live = None;
                })?;
                match result {
                    Err(NativeError::Unwind(payload)) if payload.0.is::<GracefulClose>() => {
                        Ok(Value::Null)
                    }
                    Err(error) => Err(error),
                    Ok(_) => Ok(Value::Null),
                }
            }
        }
    }

    pub(crate) async fn fiber_suspend(&mut self, value: Option<Value>) -> NativeResult {
        let error = match self.active_fiber.as_ref() {
            None => Some("Cannot suspend outside of a fiber"),
            Some(active) if active.closing => Some("Cannot suspend in a force-closed fiber"),
            Some(_) => None,
        };
        if let Some(message) = error {
            return Err(self.fiber_call_error(
                "Fiber::suspend",
                value.as_slice(),
                fiber_error(message),
            ));
        }
        let active = self
            .active_fiber
            .as_ref()
            .ok_or_else(|| internal_error!("current fiber"))?;
        let exchange = active.exchange.clone();
        exchange
            .yielded
            .set(Some(value.clone().unwrap_or_default()));
        loop {
            exchange.park_child(self)?;
            let input = poll_fn(|_| exchange.input.take().map_or(Poll::Pending, Poll::Ready)).await;
            exchange.restore_child(self)?;
            match input {
                Input::Value(value) => return Ok(value),
                Input::Throw(Value::Object(exception)) => {
                    return Err(Thrown::from_object(exception).into_error());
                }
                Input::Throw(_) | Input::Start => {
                    return Err(internal_error!("fiber resume value"));
                }
                Input::Close => {
                    if let Some(active) = &mut self.active_fiber {
                        active.closing = true;
                    }
                    return Err(GracefulClose::error());
                }
                Input::Inspect(inspection) => {
                    exchange.inspected.set(Some(
                        self.fiber_snapshot(inspection, TraceEntry::Suspend(value.as_ref())),
                    ));
                }
            }
        }
    }

    fn take_resumable(
        &mut self,
        object: &Rc<Object>,
        method: &str,
        arguments: &[Value],
    ) -> Result<Suspended, NativeError> {
        take_suspended(object).map_err(|error| self.fiber_call_error(method, arguments, error))
    }

    fn fiber_call_error(
        &mut self,
        method: &str,
        arguments: &[Value],
        error: NativeError,
    ) -> NativeError {
        phera_extension_api::Context::native_call_error(self, method, arguments, error)
    }
}

pub(super) fn take_suspended(object: &Object) -> Result<Suspended, NativeError> {
    with_state(object, |fiber| {
        let previous = std::mem::replace(&mut fiber.state, State::Running);
        match previous {
            State::Suspended(state) => Ok(state),
            other => {
                fiber.state = other;
                Err(fiber_error("Cannot resume a fiber that is not suspended"))
            }
        }
    })?
}
