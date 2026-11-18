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

use phera_extension_api::{NativeError, Unwind};
use phera_value::Value;

use crate::{Vm, exception_unwind::append_previous, exceptions::Thrown, finally::Continuation};

use super::{Generator, Item, State, activation::Activation};

#[derive(Default)]
pub(crate) struct Releases(RefCell<Vec<Released>>);

pub(super) type ReleaseQueue = Weak<Releases>;

enum Released {
    Owners(State, Option<Item>),
    Close(Activation),
}

struct ClosingComplete;

/// FINALLY_RETURN ends the forced close subroutine without executing its normal successor.
pub(crate) fn close_complete() -> NativeError {
    NativeError::Unwind(Unwind(Box::new(ClosingComplete)))
}

impl Drop for Generator {
    fn drop(&mut self) {
        if let Some(queue) = self.releases.upgrade() {
            queue.0.borrow_mut().push(Released::Owners(
                std::mem::replace(&mut self.state, State::Failed),
                self.item.take(),
            ));
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
    pub(crate) async fn drain_generator_releases(&mut self) -> Result<(), NativeError> {
        let Some(releases) = self.generator_releases.clone() else {
            return Ok(());
        };
        self.draining_generators = true;
        let result = self.run_generator_releases(&releases).await;
        self.draining_generators = false;
        result
    }

    /// Preserve the active exception unless releasing a frame raises a replacement.
    pub(crate) async fn unwind_generator_releases(&mut self, error: NativeError) -> NativeError {
        if !self.generator_releases_pending() {
            return error;
        }
        let previous = self.materialize_error(error);
        match self.drain_generator_releases().await {
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
    }

    async fn run_generator_releases(&mut self, queue: &Releases) -> Result<(), NativeError> {
        loop {
            let released = queue.0.borrow_mut().pop();
            match released {
                Some(Released::Owners(state, item)) => {
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
                }
                Some(Released::Close(activation)) => self.close_generator(activation).await?,
                None => return Ok(()),
            }
        }
    }

    async fn close_generator(&mut self, activation: Activation) -> Result<(), NativeError> {
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
        self.restore_generator_frame(activation);
        // The suspended expression's unfinished arguments cannot enter the cleanup.
        if let Some(frame) = self.current() {
            let end = frame.end();
            self.call_stack.abandon_calls_from(end);
            self.registers.truncate(end);
        }
        let result = self.run_closing_finally(handlers, stop).await;
        self.closing_generator = previous;
        self.diagnostic_reporter.restore_silence(caller_silence);
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
