//! Abandoning a suspended generator runs its pending finally bodies.
//!
//! ```php
//! function open() { try { yield; } finally { echo 'closed'; } }
//! $stream = open();
//! $stream->rewind();
//! unset($stream);
//! ```

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use phera_extension_api::{LocalFuture, NativeError, Unwind};
use phera_value::{Object, Value};

use crate::{
    Vm, diagnostics::SourceLocation, error_traces::NativeCaller, exception_unwind::append_previous,
    exceptions::Thrown, finally::Continuation,
};

use super::{Generator, Item, State, activation::Activation};

#[derive(Default)]
pub(crate) struct Releases(RefCell<Vec<Released>>);

pub(super) type ReleaseQueue = Weak<Releases>;

enum Released {
    Owners {
        state: State,
        item: Option<Item>,
        closure: Option<Rc<Object>>,
    },
    Close(Activation),
}

enum CloseSite {
    Release(Option<SourceLocation>),
    Shutdown,
}

struct ClosingComplete;

/// FINALLY_RETURN ends the forced close subroutine without executing its normal successor.
pub(crate) fn close_complete() -> NativeError {
    NativeError::Unwind(Unwind(Box::new(ClosingComplete)))
}

impl Drop for Generator {
    fn drop(&mut self) {
        if let Some(queue) = self.releases.upgrade() {
            queue.0.borrow_mut().push(self.take_released());
        }
    }
}

impl Generator {
    fn take_released(&mut self) -> Released {
        Released::Owners {
            state: std::mem::replace(&mut self.state, State::Failed),
            item: self.item.take(),
            closure: self.closure.take(),
        }
    }
}

impl Vm {
    pub(super) fn generator_release_queue(&mut self) -> ReleaseQueue {
        Rc::downgrade(self.generator_releases.get_or_insert_with(Rc::default))
    }

    #[inline]
    pub(crate) fn generator_releases_pending(&self) -> bool {
        self.generator_releases
            .as_ref()
            .is_some_and(|queue| !self.draining_generators && !queue.0.borrow().is_empty())
    }

    pub(crate) fn force_closing_generator(&self) -> bool {
        self.closing_generator == Some(self.call_stack.depth())
    }

    /// The queue keeps destruction iterative, including a chain of abandoned delegates.
    #[cold]
    pub(crate) fn drain_generator_releases(&mut self) -> LocalFuture<'_, Result<(), NativeError>> {
        self.drain_generator_releases_at(CloseSite::Release(None))
    }

    fn drain_generator_releases_at(
        &mut self,
        site: CloseSite,
    ) -> LocalFuture<'_, Result<(), NativeError>> {
        Box::pin(async move {
            let Some(releases) = self.generator_releases.clone() else {
                return Ok(());
            };
            self.draining_generators = true;
            let result = self.run_generator_releases(&releases, &site).await;
            self.draining_generators = false;
            result
        })
    }

    /// The request object store closes suspended generators even when globals
    /// or other PHP values still own them.
    pub(crate) async fn finish_request_generators(&mut self) -> Result<(), NativeError> {
        let Some(queue) = self.generator_releases.clone() else {
            return Ok(());
        };
        self.release_global_generators().await?;
        let mut index = 0;
        while let Some(entry) = self.classes.objects.get(index) {
            index += 1;
            let Some(object) = entry.upgrade().filter(super::generator_object) else {
                continue;
            };
            let released = super::state(&object, |generator| {
                matches!(
                    generator.state,
                    State::Ready(_) | State::Yielded(..) | State::Delegating(..)
                )
                .then(|| generator.take_released())
            })?;
            if let Some(released) = released {
                queue.0.borrow_mut().push(released);
                self.drain_generator_releases_at(CloseSite::Shutdown)
                    .await?;
            }
        }
        self.drain_generator_releases_at(CloseSite::Shutdown).await
    }

    /// PHP destroys sole-owned objects in reverse symbol order before it
    /// visits shared objects in the request object store.
    async fn release_global_generators(&mut self) -> Result<(), NativeError> {
        loop {
            let names: Vec<_> = self
                .globals
                .iter()
                .rev()
                .filter(|(_, value)| is_generator(value))
                .map(|(name, _)| name.clone())
                .collect();
            let mut removed = false;
            for name in names {
                if matches!(self.globals.get(&name), Some(Value::Object(object))
                    if super::generator_object(object) && Rc::strong_count(object) == 1)
                {
                    self.globals.remove(&name);
                    removed = true;
                    self.drain_generator_releases_at(CloseSite::Shutdown)
                        .await?;
                }
            }
            if !removed {
                return Ok(());
            }
        }
    }

    /// Preserve the active exception unless releasing a frame raises a replacement.
    #[cold]
    pub(crate) fn unwind_generator_releases(
        &mut self,
        error: NativeError,
    ) -> LocalFuture<'_, NativeError> {
        Box::pin(async move {
            let previous = self.materialize_error(error);
            if matches!(&previous, NativeError::Unwind(error)
                if error.0.is::<crate::last_error::FatalDiagnostic>())
            {
                return previous;
            }
            let caller = match &previous {
                NativeError::Unwind(error) if self.current().is_none() => {
                    error.0.downcast_ref::<Thrown>().map(Thrown::location)
                }
                _ => None,
            };
            match self
                .drain_generator_releases_at(CloseSite::Release(caller))
                .await
            {
                Ok(()) => previous,
                Err(error) => {
                    let replacement = self.materialize_error(error);
                    if let (NativeError::Unwind(current), NativeError::Unwind(old)) =
                        (&replacement, previous)
                        && let (Some(current), Ok(old)) = (
                            current.0.downcast_ref::<Thrown>(),
                            old.0.downcast::<Thrown>(),
                        )
                    {
                        append_previous(&current.object, old.object);
                    }
                    replacement
                }
            }
        })
    }

    async fn run_generator_releases(
        &mut self,
        queue: &Releases,
        site: &CloseSite,
    ) -> Result<(), NativeError> {
        loop {
            let released = queue.0.borrow_mut().pop();
            match released {
                Some(Released::Owners {
                    state,
                    item,
                    closure,
                }) => {
                    match state {
                        State::Yielded(activation, _) | State::Ready(activation) => {
                            queue.0.borrow_mut().push(Released::Close(activation));
                        }
                        State::Delegating(activation, _, delegate) => {
                            queue.0.borrow_mut().push(Released::Close(activation));
                            drop(delegate);
                        }
                        _ => drop(state),
                    }
                    drop(item);
                    drop(closure);
                }
                Some(Released::Close(activation)) => self.close_generator(activation, site).await?,
                None => return Ok(()),
            }
        }
    }

    async fn close_generator(
        &mut self,
        activation: Activation,
        site: &CloseSite,
    ) -> Result<(), NativeError> {
        let position = activation.frame.ip().saturating_sub(1);
        let handlers: Vec<_> = self
            .code
            .function(activation.frame.function)
            .handlers
            .iter()
            .filter(|handler| handler.start.index() <= position && position < handler.end.index())
            .filter_map(|handler| handler.finally.map(|target| (handler.exception, target)))
            .collect();
        if handlers.is_empty() {
            return Ok(());
        }
        let stop = self.call_stack.depth();
        let caller_silence = self.diagnostic_reporter.silence();
        let previous = self.closing_generator.replace(stop + 1);
        let caller = match site {
            CloseSite::Release(None) => None,
            CloseSite::Release(Some(location)) => {
                Some(NativeCaller::Internal(Some(location.clone())))
            }
            CloseSite::Shutdown => Some(NativeCaller::Internal(None)),
        };
        let traced = caller.is_some();
        if let Some(caller) = caller {
            self.push_native_callback(caller);
        }
        self.restore_generator_frame(activation);
        // The suspended expression's unfinished arguments cannot enter the cleanup.
        if let Some(frame) = self.current() {
            let end = frame.end();
            self.call_stack.abandon_calls_from(end);
            self.registers.truncate(end);
        }
        let result = self.run_closing_finally(handlers, stop).await;
        if traced {
            self.native_callback_sites.pop();
        }
        self.closing_generator = previous;
        self.restore_silence(caller_silence);
        if self.call_stack.depth() > stop {
            self.finish_return_value(Value::Null, stop)?;
        }
        result
    }

    async fn run_closing_finally(
        &mut self,
        handlers: Vec<(phera_bytecode::Reg, phera_bytecode::Target)>,
        stop: usize,
    ) -> Result<(), NativeError> {
        for (state, target) in handlers {
            self.enter_finally(state, target, Continuation::GeneratorClose)?;
            match self.interpret(stop).await {
                Err(NativeError::Unwind(ref payload)) if payload.0.is::<ClosingComplete>() => {}
                Err(error) => return Err(error),
                Ok(_) => return Ok(()),
            }
        }
        Ok(())
    }
}

fn is_generator(value: &Value) -> bool {
    matches!(value, Value::Object(object) if super::generator_object(object))
}
