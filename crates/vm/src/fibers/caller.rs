//! `$fiber->resume()` keeps its PHP caller available for an exception trace.
//! The parked call stack can be published without formatting a trace on each
//! switch. Only an exception actually asks to read it.

use std::mem::{replace, swap};

use crate::Vm;
use crate::calls::{call_stack::CallStack, registers::Registers};
use crate::error_traces::NativeCallbackSite;
use crate::generators::trace::Site;
use phera_value::Value;

pub(super) struct CallerStack {
    calls: CallStack,
    registers: Registers,
    generators: Vec<Site>,
    callbacks: Vec<NativeCallbackSite>,
}

impl CallerStack {
    pub(super) fn take(vm: &mut Vm) -> Self {
        Self {
            calls: replace(&mut vm.call_stack, CallStack::new()),
            registers: replace(&mut vm.registers, Registers::new()),
            generators: std::mem::take(&mut vm.generator_traces),
            callbacks: std::mem::take(&mut vm.native_callback_sites),
        }
    }

    pub(super) fn restore(self, vm: &mut Vm) {
        vm.call_stack = self.calls;
        vm.registers = self.registers;
        vm.generator_traces = self.generators;
        vm.native_callback_sites = self.callbacks;
    }

    pub(super) fn trace(&mut self, child: &mut Vm) -> Value {
        self.swap(child);
        let trace = child.trace_without_fiber_caller(false);
        self.swap(child);
        trace
    }

    fn swap(&mut self, vm: &mut Vm) {
        swap(&mut vm.call_stack, &mut self.calls);
        swap(&mut vm.registers, &mut self.registers);
        swap(&mut vm.generator_traces, &mut self.generators);
        swap(&mut vm.native_callback_sites, &mut self.callbacks);
    }
}
