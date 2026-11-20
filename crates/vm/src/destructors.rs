//! Running `__destruct` when the last reference to an object goes away.
//!
//! ```php
//! class Transaction {
//!     public bool $committed = false;
//!     public function __destruct() {
//!         if (!$this->committed) echo "rollback\n";
//!     }
//! }
//! function transfer() {
//!     $tx = new Transaction;
//!     throw new Exception("insufficient funds");
//! }
//! try { transfer(); } catch (Exception $e) { echo "caught\n"; }
//! // prints "rollback" while the exception leaves transfer(), then "caught"
//! ```
//!
//! An object whose class declares `__destruct` is not freed by its last
//! release: the value crate's `Drop` queues it on the request's id pool
//! (`phera_value::Destruction` explains how, without `unsafe`). This module
//! is the other half. The interpreter asks [`Vm::destructors_due`] after
//! each instruction that can release a value and, when it answers yes,
//! awaits [`Vm::run_destructors`] before the next instruction runs. So a
//! destructor runs in the frame that dropped the object, before the next
//! statement, and as ordinary PHP code: it may call anything, suspend at an
//! asynchronous native call, throw, or call `exit`.
//!
//! What PHP does, and this module copies:
//! - A destructor that throws makes the release throw. If an exception was
//!   already on its way (another destructor of the same release threw), the
//!   new one gets the old one as its previous exception.
//! - A private or protected destructor called from a scope that may not
//!   call it is not run; the release throws "Call to private D::__destruct()
//!   from global scope" instead.
//! - A destructor that stores `$this` keeps the object alive, and its
//!   destructor does not run a second time.

use std::rc::Rc;

use phera_bytecode::Visibility;
use phera_extension_api::NativeError;
use phera_value::Object;

use crate::Vm;
use crate::exceptions::Thrown;

impl Vm {
    /// Whether an object waits for its destructor. One flag test.
    #[inline]
    pub(crate) fn destructors_due(&self) -> bool {
        self.object_ids.has_doomed()
    }

    /// Calls the destructor of every released object, oldest release first,
    /// including objects released by those destructors.
    pub(crate) async fn run_destructors(&mut self) -> Result<(), NativeError> {
        Box::pin(self.collect_cycles_if_due()).await?;
        let mut failure: Option<NativeError> = None;
        loop {
            // Released generators close (run their `finally` blocks) with
            // the destructors, in particular at the end of the request.
            if self.generator_releases_pending() {
                self.drain_generator_releases().await?;
            }
            let doomed = self.classes.ids().take_doomed();
            if doomed.is_empty() {
                break;
            }
            for object in doomed {
                if let Err(error) = self.destruct(object).await {
                    failure = Some(match failure {
                        Some(earlier) => self.chain_exceptions(earlier, error),
                        None => error,
                    });
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }

    /// Unwinds `error` to its `catch` or `finally` (as `handle_exception`
    /// does), then runs the destructors of what the frames it left released,
    /// before the handler. A destructor that throws there throws from the
    /// same place, with the unwound exception as its previous one, so the
    /// same `catch` can take it, as in PHP.
    pub(crate) async fn unwind_to_handler(
        &mut self,
        error: NativeError,
        stop: usize,
    ) -> Result<(), NativeError> {
        // With no destructor due anywhere, unwinding cannot make one due.
        if !self.classes.ids().has_due() {
            return self.handle_exception(error, stop);
        }
        let mut error = error;
        loop {
            let unwound = self.materialize_error(error);
            let previous = thrown_object(&unwound);
            let positions: Vec<usize> = self
                .call_stack
                .frames()
                .iter()
                .map(|frame| frame.ip())
                .collect();
            let finallies = self.finally_stack.len();
            self.handle_exception(unwound, stop)?;
            if !self.destructors_due() {
                return Ok(());
            }
            let Err(failure) = self.run_destructors().await else {
                return Ok(());
            };
            let failure = self.materialize_error(failure);
            if let (Some(previous), Some(object)) = (previous, thrown_object(&failure)) {
                crate::exception_unwind::append_previous(&object, previous);
            }
            // Back to where the first exception was thrown, and out of a
            // `finally` that it entered: the new exception takes its place.
            self.finally_stack.truncate(finallies);
            let depth = self.call_stack.depth();
            if let (Some(&ip), Some(frame)) = (
                positions.get(depth.wrapping_sub(1)),
                self.call_stack.current_mut(),
            ) {
                frame.set_ip(ip);
            }
            error = failure;
        }
    }

    /// Calls one object's destructor, after the visibility check PHP makes.
    pub(crate) async fn destruct(&mut self, object: Rc<Object>) -> Result<(), NativeError> {
        if object.skips_destructor() {
            return Ok(());
        }
        if Vm::is_suspended_fiber(&object) {
            return Box::pin(self.close_fiber(object)).await.map(drop);
        }
        let Some(class) = self.classes.for_object(&object) else {
            return Ok(());
        };
        if let Some(release) = class.hooks.as_ref().and_then(|hooks| hooks.release) {
            release(self, &object).await;
        }
        let Some(method) = class
            .magic
            .as_ref()
            .and_then(|magic| magic.destruct.clone())
        else {
            return Ok(());
        };
        if method.visibility != Visibility::Public {
            // At the end of a request no PHP code is calling: PHP skips the
            // destructor with a warning instead.
            if self.request_end.shutting_down && self.call_stack.depth() <= 1 {
                let visibility = match method.visibility {
                    Visibility::Private => "private",
                    _ => "protected",
                };
                // PHP is running no code here, so it names no place.
                let previous =
                    self.diagnostic_location
                        .replace(crate::diagnostics::SourceLocation {
                            file: "Unknown".into(),
                            line: 0,
                        });
                self.warn(format!(
                    "Call to {visibility} {}::__destruct() from global scope during shutdown ignored",
                    class.name
                ));
                self.diagnostic_location = previous;
                return Ok(());
            }
            self.check_destructor_visibility(&method, &class.name)?;
        }
        self.invoke_magic_method(method, class.clone(), object.clone(), [])
            .await?;
        // The destructor may have stored `$this`; the object is a new `Rc`
        // (see `phera_value::Destruction`), so the request's list of objects
        // must point at this one for the end of the request.
        if Rc::strong_count(&object) > 1 {
            self.classes.track(&object);
        }
        Ok(())
    }

    /// PHP names the object's class, not the declaring one:
    /// "Call to private Child::__destruct() from global scope".
    fn check_destructor_visibility(
        &self,
        method: &crate::classes::Method,
        class: &str,
    ) -> Result<(), NativeError> {
        self.check_method_visibility(method).map_err(|_| {
            let visibility = match method.visibility {
                Visibility::Private => "private",
                _ => "protected",
            };
            let from = self.current().and_then(|frame| frame.scope()).map_or_else(
                || "global scope".to_string(),
                |scope| format!("scope {}", scope.name),
            );
            NativeError::error(format!(
                "Call to {visibility} {class}::__destruct() from {from}"
            ))
        })
    }

    /// The exception of a later destructor, with the earlier one as its
    /// previous. `exit` wins over any exception.
    fn chain_exceptions(&mut self, earlier: NativeError, later: NativeError) -> NativeError {
        let earlier = self.materialize_error(earlier);
        let later = self.materialize_error(later);
        match (thrown_object(&earlier), thrown_object(&later)) {
            (Some(previous), Some(object)) => {
                crate::exception_unwind::append_previous(&object, previous);
                later
            }
            (None, _) => earlier,
            (Some(_), None) => later,
        }
    }
}

/// The exception object an error carries, if it is a PHP exception.
fn thrown_object(error: &NativeError) -> Option<Rc<Object>> {
    match error {
        NativeError::Unwind(payload) => payload
            .0
            .downcast_ref::<Thrown>()
            .map(|thrown| thrown.object.clone()),
        _ => None,
    }
}
