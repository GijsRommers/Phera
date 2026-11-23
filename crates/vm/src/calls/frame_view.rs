//! Reading about a frame: its code, its registers, its class context.
//!
//! ```php
//! function area(): float { return $this->width * $this->height; }
//! ```
//!
//! A trace, a warning or `get_called_class()` look at a frame through a
//! [`FrameView`]: the frame's numbers together with the code table, the
//! frame's side-table entry, the registers and the class table, from which
//! it answers which function runs, at which line, with which arguments,
//! and in which class context.

use std::rc::Rc;

use phera_bytecode::{CompiledUnit, Function, Reg};
use phera_value::Value;

use crate::calls::frame::Frame;
use crate::calls::frame_extras::FrameExtras;
use crate::calls::registers::Registers;
use crate::calls::window_context::WindowContext;
use crate::classes::{Class, ClassTable, MethodContext};
use crate::code_table::CodeTable;
use crate::unit_caches::UnitCache;

/// A frame together with everything needed to read about it: its code and
/// its side-table entry. Handlers get one from `Vm::current`.
#[derive(Clone, Copy)]
pub(crate) struct FrameView<'a> {
    pub(crate) frame: Frame,
    pub(crate) code: &'a CodeTable,
    pub(crate) extras: &'a FrameExtras,
    pub(crate) registers: &'a Registers,
    pub(crate) classes: &'a ClassTable,
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

    #[inline]
    pub(crate) fn cache(&self) -> &'a UnitCache {
        self.code.cache(self.frame.function)
    }

    /// File and function scopes both expose their compiled and dynamic names.
    pub(crate) fn named_registers(&self) -> impl Iterator<Item = (&'a String, Reg)> {
        let names = &self.unit().names;
        self.function()
            .locals
            .iter()
            .enumerate()
            .filter_map(move |(index, name)| Some((names.get(name.index())?, Reg(index as u16))))
            .chain(
                self.extras
                    .dynamic
                    .iter()
                    .map(|(name, register)| (name, *register)),
            )
    }

    pub(crate) fn named_register(&self, name: &str) -> Option<Reg> {
        self.function()
            .locals
            .iter()
            .position(|candidate| {
                self.unit()
                    .names
                    .get(candidate.index())
                    .is_some_and(|value| value == name)
            })
            .map(|index| Reg(index as u16))
            .or_else(|| self.extras.dynamic.get(name).copied())
    }

    /// The function's number within its file; `None` for top-level code.
    pub(crate) fn function_id(&self) -> Option<usize> {
        self.code.local(self.frame.function)
    }

    /// What the frame's window holds below its first register: `$this`, the
    /// closure, and from them the class scope and the called class.
    pub(crate) fn context(&self) -> WindowContext<'a> {
        WindowContext {
            kind: self.frame.kind(),
            function: self.frame.function,
            base: self.base(),
            called: self.extras.called,
            code: self.code,
            registers: self.registers,
            classes: self.classes,
        }
    }

    /// `$this` of a method or closure frame.
    pub(crate) fn this(&self) -> Option<&'a Rc<phera_value::Object>> {
        self.context().this()
    }

    /// The closure object a closure frame runs.
    pub(crate) fn closure(&self) -> Option<&'a Rc<phera_value::Object>> {
        self.context().closure()
    }

    /// The class scope (`self::`).
    pub(crate) fn scope(&self) -> Option<&'a Rc<Class>> {
        self.context().scope()
    }

    /// The called class (`static::`).
    pub(crate) fn called(&self) -> Option<Rc<Class>> {
        self.context().called()
    }

    /// The class context of a method or closure frame.
    pub(crate) fn method(&self) -> Option<MethodContext> {
        self.context().method()
    }

    /// How many registers belong to the frame: its declared registers plus
    /// variables created by name at run time.
    pub(crate) fn count(&self) -> usize {
        // The facts hold the count, one load away; the function itself is
        // reached through its file.
        self.extras
            .dynamic_count
            .unwrap_or(self.code.facts(self.frame.function).registers as usize)
    }

    /// Local operands address only this activation, excluding saved arguments.
    #[inline]
    pub(crate) fn slots<'r>(&self, registers: &'r Registers) -> &'r [Option<Value>] {
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
            .or(self.frame.passed())
            .unwrap_or(self.function().parameters.len())
    }

    /// What `func_num_args()` reports.
    pub(crate) fn argument_count(&self) -> usize {
        self.extras
            .arguments
            .count
            .or(self.frame.passed())
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
}
