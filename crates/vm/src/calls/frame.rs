//! A frame is five numbers; what only some frames need lives beside it.
//!
//! During `f(1, g(2))`, after `g` has been entered:
//!
//! ```text
//! frames (Vec<Frame>):   [ {main} base 0 ]  [ g base 9, destination r6 ]
//! registers:             [ main's locals and temporaries | g: 2, g's locals ]
//!                          0                                9
//! side table (by depth):   0: dynamic names of {main}       1: (unused)
//! ```
//!
//! `f`'s first argument is already in main's temporaries; when `g` returns, its
//! result goes to main's r6 and `g`'s registers are emptied again. Pushing
//! and popping a frame copies sixteen bytes. A frame that uses `foreach`,
//! `$$name`, `func_get_args()`, a bound closure or `$this` keeps that state in
//! [`FrameExtras`] at its depth, and only such frames touch it.

use std::rc::Rc;

use phera_bytecode::{CompiledUnit, Constant, Function, Reg};
use phera_value::{PhpArray, Value};

use crate::AccessPath;
use crate::calls::frame_extras::FrameExtras;
use crate::code_table::{CodeTable, FunctionIndex};
use crate::unit_caches::UnitCache;

/// Foreach either owns a value snapshot or follows a live reference path.
pub(crate) struct IteratorState {
    /// By-value foreach keeps this owner to separate subsequent array writes.
    pub(crate) snapshot: Option<Rc<PhpArray>>,
    /// By-reference foreach resolves this place again as the array changes.
    pub(crate) source: Option<AccessPath>,
    /// The array storage position, distinct from a PHP integer key.
    pub(crate) position: Option<usize>,
    /// Distinguishes an untouched iterator from an exhausted one.
    pub(crate) started: bool,
}

/// One running PHP function: which code, where its registers start, where it
/// continues, and where its result goes.
#[derive(Clone, Copy)]
pub(crate) struct Frame {
    pub(crate) function: FunctionIndex,
    base: u32,
    ip: u32,
    /// The caller's register that receives the result.
    destination: Reg,
    flags: FrameFlags,
}

/// Small facts about a frame, one bit each.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FrameFlags(u16);

impl FrameFlags {
    /// The frame's side-table entry holds something and is emptied on return.
    const EXTRAS: u16 = 1;
    /// Native code called this function: its result goes back to Rust, not
    /// to a register.
    const CALLBACK: u16 = 1 << 1;
}

const _: () = assert!(std::mem::size_of::<Frame>() == 16);

impl Frame {
    /// A frame about to run `function` from its first instruction.
    pub(crate) fn new(function: FunctionIndex, base: usize, destination: Option<Reg>) -> Self {
        let mut flags = FrameFlags::default();
        if destination.is_none() {
            flags.0 |= FrameFlags::CALLBACK;
        }
        Self {
            function,
            base: base as u32,
            ip: 0,
            destination: destination.unwrap_or(Reg(0)),
            flags,
        }
    }

    /// Where this frame's registers start in the request's register stack.
    #[inline]
    pub(crate) fn base(&self) -> usize {
        self.base as usize
    }

    /// The next instruction to run.
    #[inline]
    pub(crate) fn ip(&self) -> usize {
        self.ip as usize
    }

    #[inline]
    pub(crate) fn set_ip(&mut self, ip: usize) {
        self.ip = ip as u32;
    }

    /// The same frame with its registers starting at `base` instead.
    pub(crate) fn moved_to(self, base: usize) -> Self {
        Self {
            base: base as u32,
            ..self
        }
    }

    /// The caller's register for the result; `None` when native code called.
    #[inline]
    pub(crate) fn destination(&self) -> Option<Reg> {
        (self.flags.0 & FrameFlags::CALLBACK == 0).then_some(self.destination)
    }

    /// Whether the side table holds state for this frame.
    #[inline]
    pub(crate) fn has_extras(&self) -> bool {
        self.flags.0 & FrameFlags::EXTRAS != 0
    }

    pub(crate) fn mark_extras(&mut self) {
        self.flags.0 |= FrameFlags::EXTRAS;
    }
}

/// A frame together with everything needed to read about it: its code and
/// its side-table entry. Handlers get one from `Vm::current`.
#[derive(Clone, Copy)]
pub(crate) struct FrameView<'a> {
    pub(crate) frame: Frame,
    pub(crate) code: &'a CodeTable,
    pub(crate) extras: &'a FrameExtras,
}

impl<'a> FrameView<'a> {
    pub(crate) fn base(&self) -> usize {
        self.frame.base()
    }

    pub(crate) fn ip(&self) -> usize {
        self.frame.ip()
    }

    pub(crate) fn function(&self) -> &'a Function {
        self.code.function(self.frame.function)
    }

    pub(crate) fn unit(&self) -> &'a std::sync::Arc<CompiledUnit> {
        self.code.unit(self.frame.function)
    }

    pub(crate) fn cache(&self) -> &'a UnitCache {
        self.code.cache(self.frame.function)
    }

    /// The function's number within its file; `None` for top-level code.
    pub(crate) fn function_id(&self) -> Option<usize> {
        self.code.local(self.frame.function)
    }

    /// The class context of a method frame.
    pub(crate) fn method(&self) -> Option<&'a crate::classes::MethodContext> {
        self.extras.method.as_ref()
    }

    /// The closure object a frame runs, which keeps its captures alive.
    pub(crate) fn closure(&self) -> Option<&'a Rc<phera_value::Object>> {
        self.extras.closure.as_ref()
    }

    /// How many registers belong to the frame: its declared registers plus
    /// variables created by name at run time.
    pub(crate) fn count(&self) -> usize {
        self.extras
            .dynamic_count
            .unwrap_or(self.function().registers as usize)
    }

    /// Local operands address only this activation, excluding saved arguments.
    #[inline]
    pub(crate) fn slots<'r>(
        &self,
        registers: &'r crate::calls::registers::Registers,
    ) -> &'r [Option<Value>] {
        registers.window(self.base(), self.count())
    }

    /// Ordinary reads see through PHP references without copying the reference cell.
    #[inline]
    pub(crate) fn get(
        &self,
        registers: &crate::calls::registers::Registers,
        register: Reg,
    ) -> Value {
        self.slots(registers)
            .get(register.index())
            .and_then(Option::as_ref)
            .map(|value| value.deref().clone())
            .unwrap_or_default()
    }

    /// PHP exposes current fixed parameters, but keeps original variadic arguments separately.
    pub(crate) fn argument(
        &self,
        registers: &crate::calls::registers::Registers,
        index: usize,
    ) -> Value {
        let function = self.function();
        let fixed = function.parameters.len()
            - usize::from(function.argument_layout == phera_bytecode::ArgumentLayout::Variadic);
        let arguments = &self.extras.arguments;
        let slot = if index < fixed {
            self.base() + index
        } else if let Some(base) = arguments.saved {
            base + index
        } else {
            self.base() + self.count() + index - fixed
        };
        registers
            .window(slot, 1)
            .first()
            .and_then(Option::as_ref)
            .map(|value| value.deref().clone())
            .unwrap_or_default()
    }

    /// How many arguments the caller passed, for traces and `func_num_args()`.
    pub(crate) fn supplied_arguments(&self) -> usize {
        self.extras
            .arguments
            .supplied
            .unwrap_or(self.function().parameters.len())
    }

    /// What `func_num_args()` reports.
    pub(crate) fn argument_count(&self) -> usize {
        self.extras
            .arguments
            .count
            .unwrap_or(self.function().parameters.len())
    }

    /// The first register past everything this frame owns: its registers,
    /// surplus arguments and saved argument copies.
    pub(crate) fn end(&self) -> usize {
        let arguments = &self.extras.arguments;
        match arguments.saved {
            Some(saved) => saved + self.argument_count(),
            None => self.base() + self.count() + arguments.extra,
        }
    }

    /// IP has already advanced, so diagnostics use the previous instruction line.
    pub(crate) fn line(&self) -> usize {
        if self.ip() == 0 && self.function_id().is_some() {
            return self.function().definition_line as usize;
        }
        self.function()
            .lines
            .get(self.ip().saturating_sub(1))
            .copied()
            .unwrap_or(1) as usize
    }

    /// The unit-owned slot is keyed by this function and its published next IP.
    pub(crate) fn site_cache(
        &self,
    ) -> Option<&'a std::cell::RefCell<crate::unit_caches::CachedSite>> {
        self.cache().site(self.function_id(), self.ip())
    }
}

/// Materialize bytecode constants as request-local PHP values.
pub(crate) fn value(constant: &Constant) -> Value {
    match constant {
        Constant::Null => Value::Null,
        Constant::Array(entries) => {
            let mut array = PhpArray::new();
            for (key, constant) in entries {
                let value = value(constant);
                match key {
                    phera_bytecode::ConstantKey::Next => {
                        let _ = array.push(value);
                    }
                    phera_bytecode::ConstantKey::Int(key) => {
                        array.insert(phera_value::ArrayKey::Int(*key), value);
                    }
                    phera_bytecode::ConstantKey::String(key) => {
                        array.insert(phera_value::ArrayKey::String(key.clone().into()), value);
                    }
                }
            }
            Value::Array(Rc::new(array))
        }
        Constant::Bool(value) => Value::Bool(*value),
        Constant::Int(value) => Value::Int(*value),
        Constant::Float(value) => Value::Float(*value),
        Constant::String(value) => Value::from(value.clone()),
    }
}
