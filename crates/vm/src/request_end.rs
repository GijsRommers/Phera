//! Destroying what is left when a request ends.
//!
//! ```php
//! class D { function __construct(public $n) {} function __destruct() { echo "d$this->n\n"; } }
//! $a = new D('a');
//! $b = new D('b');
//! $shared = new D('shared'); $alias = $shared;
//! $list = [new D('in-array')];
//! echo "end\n";
//! // end, db, da, dshared, din-array
//! ```
//!
//! After the script's last statement, after `exit`, and after an uncaught
//! exception, PHP (`php_request_shutdown`) does, in this order:
//!
//! 1. Runs the shutdown functions.
//! 2. Destroys the global variables that hold an object nobody else holds,
//!    from the last global to the first, each destructor running as its
//!    variable goes. A destructor may create globals; PHP repeats the pass
//!    until it changes nothing. Globals holding a PHP reference, an array or
//!    a shared object are left alone.
//!    A suspended fiber is destroyed the same way: it is unwound.
//! 3. Calls the destructor of every object still alive, in the order the
//!    objects were created (their ids). Objects that destructors create in
//!    this phase are included, and from here on ids are no longer reused.
//! 4. Flushes the output buffers.
//!
//! A destructor that throws in phase 2 or 3 ends the request with "Fatal
//! error: Uncaught ...", and no other destructor runs; `exit` in one ends it
//! silently. After a fatal error (as opposed to an uncaught exception) no
//! object alive at that moment gets its destructor, but the objects that
//! shutdown functions create do, as in any request. Whatever is still alive
//! afterwards is freed without its destructor: the id pool is closed.
//!
//! The main script's registers publish their bindings to the ordered request
//! symbol table before the stack is released. Shutdown callbacks and
//! destructors both see that table.

use phera_extension_api::NativeError;
use phera_value::Destruction;

use crate::Vm;

/// The payload of the error that `exit` unwinds with.
pub(crate) struct Exit;

/// The payload of a fatal error that is not an exception (`E_USER_ERROR`):
/// the request ends without destructors.
pub(crate) struct FatalStop;

/// How the script's code ended.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScriptEnd {
    /// The last statement ran, `exit` was called, or an exception was
    /// uncaught: destructors run.
    Destructible,
    /// A fatal error: no destructor runs.
    Fatal,
}

/// The request's state for its end.
#[derive(Default)]
pub(crate) struct RequestEnd {
    /// The destructor phases have begun: PHP runs destructors there with no
    /// calling code, so a private destructor is skipped with a warning.
    pub(crate) shutting_down: bool,
}

impl ScriptEnd {
    /// What the error that ended the script means for its end.
    pub(crate) fn of(error: &NativeError) -> Self {
        match error {
            NativeError::Unwind(payload)
                if payload.0.is::<FatalStop>()
                    || payload.0.is::<crate::error_traces::DeclarationFailure>()
                    || payload.0.is::<crate::last_error::FatalDiagnostic>() =>
            {
                ScriptEnd::Fatal
            }
            _ => ScriptEnd::Destructible,
        }
    }
}

impl Vm {
    /// `exit` leaves every function: their variables are released, the
    /// innermost frame first. File-scope symbols stay in the request table.
    pub(crate) fn release_frames_for_exit(&mut self) {
        self.park_global_registers();
        while let Some(frame) = self.call_stack.pop() {
            self.call_stack.abandon_calls_from(frame.start());
            self.closure_captures
                .retain(|capture| capture.base < frame.base());
            self.registers.release_frame(frame.start(), frame.base());
        }
    }

    /// Ends the request in PHP's order; `end` says how the script's code
    /// ended.
    pub(crate) async fn finish_request(&mut self, end: ScriptEnd) {
        if end == ScriptEnd::Fatal {
            self.fiber_bailout = true;
            self.forget_destructors();
        }
        // 1. Shutdown functions; they run after a fatal error too.
        self.run_shutdown_callbacks().await;
        // 2. Destructors.
        self.run_request_destructors().await;
        // 3. Output buffers. Objects their callbacks create still get
        //    destructors, since the pool is not closed yet.
        self.finish_buffers().await;
        // 4. Everything still alive is freed without its destructor.
        drop(self.classes.ids().close());
        self.call_stack.clear();
        self.registers.truncate(0);
    }

    /// A fatal error: PHP marks every object alive as destructed, and those
    /// already released wait for no destructor either.
    ///
    /// ```php
    /// $alive = new D;                  // never destructed
    /// register_shutdown_function(fn () => $GLOBALS['late'] = new D);
    /// eval('function f() {}'); eval('function f() {}');   // fatal
    /// // the shutdown function runs; $late is destructed at the end
    /// ```
    fn forget_destructors(&mut self) {
        for object in self.classes.live_objects() {
            if object.destruction() == Destruction::Due {
                object.mark_destructed();
            }
        }
        // Marked first, so that nothing these free is queued again.
        drop(self.object_ids.take_doomed());
    }

    /// Shutdown and destructor callbacks share the parked request globals.
    async fn run_request_destructors(&mut self) {
        self.call_stack.clear();
        self.request_end.shutting_down = true;
        let outcome = match self.destroy_globals().await {
            Ok(()) => self.destroy_remaining_objects().await,
            failure => failure,
        };
        if let Err(error) = outcome {
            let error = self.materialize_error(error);
            self.unwind(error);
        }
        self.request_end.shutting_down = false;
        // Output buffers that are flushed next run with no PHP code
        // calling, as PHP's do ("in Unknown on line 0").
        self.call_stack.clear();
    }

    /// Phase 2: globals holding the only reference to an object, last to
    /// first, until a pass changes nothing.
    async fn destroy_globals(&mut self) -> Result<(), NativeError> {
        self.run_destructors().await?;
        if !self.end_phases_have_work() {
            return Ok(());
        }
        loop {
            let before = self.globals.iter().count();
            for name in self.globals.sole_object_owners() {
                self.globals.release_sole_object(&name);
                self.run_destructors().await?;
            }
            if self.globals.iter().count() == before {
                return Ok(());
            }
        }
    }

    /// Whether the destructor phases can do anything: some object has a
    /// destructor due, or a generator exists (destroying one runs its
    /// `finally` blocks).
    fn end_phases_have_work(&self) -> bool {
        self.object_ids.has_due() || self.generator_releases.is_some()
    }

    /// Phase 3: every object whose destructor has not run, by id, including
    /// objects created meanwhile.
    async fn destroy_remaining_objects(&mut self) -> Result<(), NativeError> {
        if !self.end_phases_have_work() {
            return Ok(());
        }
        self.classes.ids().stop_reuse();
        let mut index = 0;
        while let Some(object) = self.classes.objects.get(index) {
            index += 1;
            let Some(object) = object.upgrade() else {
                continue;
            };
            if crate::generators::generator_object(&object) {
                // A generator is destroyed here too: its `finally` runs.
                self.close_live_generator(&object).await?;
                self.run_destructors().await?;
                continue;
            }
            if object.destruction() != Destruction::Due {
                continue;
            }
            object.mark_destructed();
            self.destruct(object).await?;
            self.run_destructors().await?;
        }
        Ok(())
    }
}
