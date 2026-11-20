//! `$fiber->resume()` gives a parked stack the request data before polling
//! its pending native and PHP frames. `Fiber::suspend()` gives it back.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use phera_extension_api::{LocalFuture, NativeError, NativeResult, fiber::FiberInspection};
use phera_value::{Value, WeakObject};

use super::caller::CallerStack;
use super::trace::{Site, Waiting};
use crate::Vm;

pub(super) enum Input {
    Start,
    Value(Value),
    Throw(Value),
    Close,
    Inspect(FiberInspection),
}

pub(super) struct Exchange {
    shared: Cell<Option<Box<Vm>>>,
    pub(super) input: Cell<Option<Input>>,
    pub(super) yielded: Cell<Option<Value>>,
    pub(super) site: RefCell<Option<Site>>,
    pub(super) caller: RefCell<Option<CallerStack>>,
    pub(super) waiting: RefCell<Option<Waiting>>,
    pub(super) inspected: Cell<Option<NativeResult>>,
    pub(super) caller_depth: Cell<usize>,
    pub(super) parked_depth: Cell<usize>,
    reporting: Cell<i64>,
}

pub(crate) struct Active {
    /// The fiber itself; weak, and still found while the fiber is being
    /// destroyed (see `phera_value::WeakObject`).
    pub(super) object: WeakObject,
    pub(super) exchange: Rc<Exchange>,
    pub(super) closing: bool,
}

pub(super) struct Suspended {
    pub(super) exchange: Rc<Exchange>,
    pub(super) future: LocalFuture<'static, NativeResult>,
}

impl Exchange {
    pub(super) fn new(request: &Vm) -> Rc<Self> {
        let shared = Box::new(request.new_fiber_stack());
        Rc::new(Self {
            shared: Cell::new(Some(shared)),
            input: Cell::new(None),
            yielded: Cell::new(None),
            site: RefCell::new(None),
            caller: RefCell::new(None),
            waiting: RefCell::new(None),
            inspected: Cell::new(None),
            caller_depth: Cell::new(0),
            parked_depth: Cell::new(0),
            reporting: Cell::new(request.fiber_initial_reporting()),
        })
    }

    pub(super) fn transfer(&self, vm: &mut Vm) -> Result<(), NativeError> {
        let mut shared = self
            .shared
            .take()
            .ok_or_else(|| internal_error!("fiber request slot"))?;
        vm.swap_fiber_request(&mut shared);
        self.shared.set(Some(shared));
        Ok(())
    }

    pub(super) fn restore_child(&self, child: &mut Vm) -> Result<(), NativeError> {
        self.transfer(child)?;
        child.error_handlers.reporting = self.reporting.get();
        child
            .call_stack
            .set_callback_depth(self.caller_depth.get() + self.parked_depth.get());
        Ok(())
    }

    pub(super) fn park_child(&self, child: &mut Vm) -> Result<(), NativeError> {
        self.reporting.set(child.error_handlers.reporting);
        let depth = child
            .call_stack
            .callback_depth()
            .checked_sub(self.caller_depth.get())
            .ok_or_else(|| internal_error!("fiber native callback depth"))?;
        self.parked_depth.set(depth);
        child.call_stack.set_callback_depth(depth);
        self.transfer(child)
    }
}

impl Vm {
    pub(super) fn check_fiber_stack_size(&self) -> Result<(), NativeError> {
        let Some(value) = self.ini_override(b"fiber.stack_size") else {
            return Ok(());
        };
        let (size, _) = phera_extension_api::ini::parse_quantity(value.as_bytes());
        let minimum = rustix::param::page_size().saturating_mul(2);
        if size < minimum as i64 {
            return Err(NativeError::throw(
                "Exception",
                format!("Fiber stack size is too small, it needs to be at least {minimum} bytes"),
            ));
        }
        Ok(())
    }

    pub(super) fn fiber_initial_reporting(&self) -> i64 {
        let value = self.ini_get(b"error_reporting");
        value
            .filter(|value| !value.is_empty())
            .map_or(30719, |value| {
                i64::from(phera_extension_api::ini::integer(value.as_bytes()) as i32)
            })
    }

    /// Only request-wide owners travel. A parked stack retains all active
    /// call and unwind state and can keep borrowing its own VM.
    fn swap_fiber_request(&mut self, other: &mut Self) {
        use std::mem::swap;
        swap(&mut self.classes, &mut other.classes);
        swap(&mut self.enum_cases, &mut other.enum_cases);
        swap(
            &mut self.unset_typed_properties,
            &mut other.unset_typed_properties,
        );
        swap(&mut self.prelinked_classes, &mut other.prelinked_classes);
        swap(&mut self.function_table, &mut other.function_table);
        swap(&mut self.code, &mut other.code);
        swap(&mut self.globals, &mut other.globals);
        swap(&mut self.static_variables, &mut other.static_variables);
        // Static properties and evaluated constants, and what
        // `Phera\snapshot()` does, belong to the request too.
        swap(&mut self.class_states, &mut other.class_states);
        swap(&mut self.snapshot, &mut other.snapshot);
        swap(&mut self.include_state, &mut other.include_state);
        swap(&mut self.generator_releases, &mut other.generator_releases);
        swap(
            &mut self.executed_instructions,
            &mut other.executed_instructions,
        );
        swap(&mut self.output, &mut other.output);
        swap(&mut self.output_writer, &mut other.output_writer);
        swap(&mut self.request_callbacks, &mut other.request_callbacks);
        swap(&mut self.constants, &mut other.constants);
        swap(&mut self.frame_limit, &mut other.frame_limit);
        swap(&mut self.status, &mut other.status);
        swap(&mut self.fiber_bailout, &mut other.fiber_bailout);
        swap(
            &mut self.error_handlers.exception,
            &mut other.error_handlers.exception,
        );
        swap(
            &mut self.error_handlers.exception_history,
            &mut other.error_handlers.exception_history,
        );
        swap(
            &mut self.error_handlers.error,
            &mut other.error_handlers.error,
        );
        swap(
            &mut self.error_handlers.error_history,
            &mut other.error_handlers.error_history,
        );
        swap(
            &mut self.error_handlers.last_error,
            &mut other.error_handlers.last_error,
        );
        swap(
            &mut self.error_handlers.fatal_trace,
            &mut other.error_handlers.fatal_trace,
        );
        swap(
            &mut self.error_handlers.fatal_error_backtraces,
            &mut other.error_handlers.fatal_error_backtraces,
        );
        swap(
            &mut self.error_handlers.trace_ignore_args,
            &mut other.error_handlers.trace_ignore_args,
        );
        swap(
            &mut self.error_handlers.trace_string_limit,
            &mut other.error_handlers.trace_string_limit,
        );
        swap(&mut self.state, &mut other.state);
        swap(&mut self.reflection_cache, &mut other.reflection_cache);
        swap(
            &mut self.property_references,
            &mut other.property_references,
        );
        swap(&mut self.autoload, &mut other.autoload);
        swap(&mut self.ini_overrides, &mut other.ini_overrides);
        swap(&mut self.suspended_fibers, &mut other.suspended_fibers);
        // The request's object pool: destructors become due and run inside
        // the fiber too. The collector's counters and the end-of-request
        // state belong to the request as well.
        swap(&mut self.object_ids, &mut other.object_ids);
        swap(&mut self.cycles, &mut other.cycles);
        swap(&mut self.request_end, &mut other.request_end);
        self.refresh_pending_callbacks();
    }
}
