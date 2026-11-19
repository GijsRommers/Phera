//! `Fiber::suspend('ready')` parks an owned interpreter future. It can
//! receive a value, an exception, or a graceful close at the same call site.

use std::future::poll_fn;
use std::rc::Rc;
use std::task::Poll;

use phera_extension_api::fiber::{FiberInspection, FiberOperation, fiber_error};
use phera_extension_api::{NativeError, NativeResult, Unwind};
use phera_value::{Object, Value};

use crate::{Vm, exceptions::Thrown};

use super::{
    Fiber, State,
    stack::{Active, Exchange, Input, Suspended},
    trace::{Method, Site},
    with_state,
};

pub(crate) struct GracefulClose;

impl GracefulClose {
    pub(crate) fn error() -> NativeError {
        NativeError::Unwind(Unwind(Box::new(Self)))
    }
}

enum Progress {
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
                .await?;
            let mut state = object
                .native
                .try_borrow_mut()
                .map_err(|_| internal_error!("fiber constructor state borrowed"))?;
            if state.is_some() {
                return Err(fiber_error("Cannot call constructor twice"));
            }
            *state = Some(Box::new(Fiber {
                callable,
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
                let callee = with_state(&object, |fiber| match fiber.state {
                    State::Ready(_) => match std::mem::replace(&mut fiber.state, State::Running) {
                        State::Ready(callee) => Ok(callee),
                        _ => Err(internal_error!("fiber start state")),
                    },
                    _ => Err(fiber_error(
                        "Cannot start a fiber that has already been started",
                    )),
                })??;
                let site = Site::from_caller(self, Method::Start, &arguments);
                let exchange = Exchange::new(self);
                let mut child = Box::new(Vm::new_shell(self.registry.clone()));
                child.active_fiber = Some(Box::new(Active {
                    object: Rc::downgrade(&object),
                    exchange: exchange.clone(),
                    closing: false,
                }));
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
                let site = Site::with_value(self, Method::Resume, &value)?;
                (take_suspended(&object)?, Input::Value(value), Some(site))
            }
            FiberOperation::Throw(value) => {
                let site = Site::with_value(self, Method::Throw, &value)?;
                (take_suspended(&object)?, Input::Throw(value), Some(site))
            }
            FiberOperation::Construct(_) => {
                return Err(internal_error!("fiber constructor routing"));
            }
        };
        suspended.exchange.site.replace(method);
        let progress = self.poll_fiber(&object, &mut suspended, input).await?;
        self.finish_fiber_progress(object, suspended, progress)
    }

    async fn poll_fiber(
        &mut self,
        object: &Object,
        suspended: &mut Suspended,
        input: Input,
    ) -> Result<Progress, NativeError> {
        self.suspended_fibers.remove(&object.id);
        let exchange = &suspended.exchange;
        let depth = self
            .active_fiber
            .as_ref()
            .map_or(1, |active| active.exchange.depth.get() + 1);
        exchange.depth.set(depth);
        exchange.input.set(Some(input));
        exchange.transfer(self)?;
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
        exchange.transfer(self)?;
        Ok(progress)
    }

    fn finish_fiber_progress(
        &mut self,
        object: Rc<Object>,
        suspended: Suspended,
        progress: Progress,
    ) -> NativeResult {
        match progress {
            Progress::Yielded(value) => {
                with_state(&object, |fiber| fiber.state = State::Suspended(suspended))?;
                self.suspended_fibers.insert(object.id, object);
                Ok(value)
            }
            Progress::Inspected(result) => {
                with_state(&object, |fiber| fiber.state = State::Suspended(suspended))?;
                self.suspended_fibers.insert(object.id, object);
                result
            }
            Progress::Finished(result) => {
                let state = match &result {
                    Ok(value) => State::Returned(value.clone()),
                    Err(NativeError::Unwind(payload)) if payload.0.is::<()>() => State::Exited,
                    Err(_) => State::Failed,
                };
                with_state(&object, |fiber| fiber.state = state)?;
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

    pub(crate) async fn fiber_suspend(&mut self, value: Value) -> NativeResult {
        let active = self
            .active_fiber
            .as_ref()
            .ok_or_else(|| fiber_error("Cannot suspend outside of a fiber"))?;
        if active.closing {
            return Err(fiber_error("Cannot suspend in a force-closed fiber"));
        }
        let exchange = active.exchange.clone();
        exchange.yielded.set(Some(value.clone()));
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
                    exchange
                        .inspected
                        .set(Some(self.fiber_snapshot(inspection, Some(&value))));
                }
            }
        }
    }

    pub(crate) async fn fiber_inspect(
        &mut self,
        object: Rc<Object>,
        inspection: FiberInspection,
    ) -> NativeResult {
        if self
            .current_fiber()
            .is_some_and(|current| Rc::ptr_eq(&object, &current))
        {
            return self.fiber_snapshot(inspection, None);
        }
        let mut suspended =
            take_suspended(&object).map_err(|_| super::reflection::invalid_state())?;
        let progress = self
            .poll_fiber(&object, &mut suspended, Input::Inspect(inspection))
            .await?;
        self.finish_fiber_progress(object, suspended, progress)
    }

    pub(crate) async fn close_fibers(&mut self) {
        while let Some((_, object)) = self.suspended_fibers.pop_last() {
            let Ok(mut suspended) = take_suspended(&object) else {
                continue;
            };
            suspended.exchange.site.replace(None);
            let result = match self.poll_fiber(&object, &mut suspended, Input::Close).await {
                Ok(progress) => self.finish_fiber_progress(object, suspended, progress),
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                self.report_uncaught(error).await;
            }
        }
    }
}

fn take_suspended(object: &Object) -> Result<Suspended, NativeError> {
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
