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
//! The variables of the main script are its frame's registers. They are not
//! released when the script's code ends, so that phase 2 can visit them; the
//! main frame is put back on the call stack for the end of the request, so
//! that destructors see the globals.

use std::rc::Rc;

use phera_extension_api::NativeError;
use phera_value::{Destruction, Value};

use crate::Vm;
use crate::calls::frame::Frame;
use crate::code_table::FunctionIndex;

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
#[derive(Default, Clone)]
pub(crate) struct RequestEnd {
    /// The main script's code; its frame keeps its registers until the end.
    pub(crate) main: Option<FunctionIndex>,
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
    /// Empties the registers of a frame that ends, except those of the main
    /// script, which are the globals and live until the end of the request.
    pub(crate) fn release_frame_registers(&mut self, frame: &Frame) {
        if frame.start() == 0 && Some(frame.function) == self.request_end.main {
            return;
        }
        self.registers.release_frame(frame.start(), frame.base());
    }

    /// An exception leaves the main script: its temporaries are released
    /// on the way out, as PHP frees an unwound frame's live temporaries,
    /// before the uncaught exception is reported. Its variables stay.
    ///
    /// ```php
    /// Missing::$property = new Owner;   // Owner goes before "Fatal error"
    /// ```
    pub(crate) fn release_main_temporaries(&mut self, frame: &Frame) {
        if frame.start() != 0 || Some(frame.function) != self.request_end.main {
            return;
        }
        let variables = self.code.function(frame.function).locals.len();
        self.registers.truncate(frame.base() + variables);
    }

    /// The main script's variable in `register` was unset while it shared a
    /// cell with the globals table: the global goes too.
    pub(crate) fn forget_global_cell(
        &mut self,
        register: phera_bytecode::Reg,
        cell: &Rc<std::cell::RefCell<Value>>,
    ) {
        let Some(frame) = self.current() else {
            return;
        };
        if frame.frame.start() != 0 || Some(frame.frame.function) != self.request_end.main {
            return;
        }
        let name = frame
            .function()
            .locals
            .get(register.index())
            .and_then(|name| frame.unit().names.get(name.index()))
            .cloned();
        if let Some(name) = name {
            self.globals.forget_cell(&name, cell);
        }
    }

    /// `exit` leaves every function: their variables are released, the
    /// innermost frame first. The main script's variables stay for the end
    /// of the request.
    pub(crate) fn release_frames_for_exit(&mut self) {
        while let Some(frame) = self.call_stack.pop() {
            self.call_stack.abandon_calls_from(frame.start());
            self.closure_captures
                .retain(|capture| capture.base < frame.base());
            self.release_frame_registers(&frame);
        }
    }

    /// Ends the request in PHP's order; `end` says how the script's code
    /// ended.
    pub(crate) async fn finish_request(&mut self, end: ScriptEnd) {
        if end == ScriptEnd::Fatal {
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

    /// The destructor phases, with the main script's frame back on the
    /// call stack so that destructors see the globals.
    async fn run_request_destructors(&mut self) {
        self.call_stack.clear();
        if let Some(main) = self.request_end.main {
            let mut frame = Frame::new(main, 0, None);
            frame.set_ip(self.code.function(main).instructions.len());
            self.call_stack.push(frame);
        }
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
        let count = self
            .request_end
            .main
            .map_or(0, |main| self.code.function(main).locals.len());
        loop {
            let before = self.defined_globals(count);
            for name in self.globals.sole_object_owners() {
                self.globals.remove(&name);
                self.run_destructors().await?;
            }
            for index in (0..count).rev() {
                let sole_owner = matches!(
                    self.registers.window(index, 1),
                    [Some(Value::Object(object))] if Rc::strong_count(object) == 1
                );
                if sole_owner {
                    drop(self.registers.take(index));
                    self.run_destructors().await?;
                }
            }
            if self.defined_globals(count) == before {
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

    /// How many globals exist, PHP's test for "the pass changed something".
    fn defined_globals(&self, count: usize) -> usize {
        let variables = self
            .registers
            .window(0, count)
            .iter()
            .filter(|slot| slot.is_some())
            .count();
        variables + self.globals.len()
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
