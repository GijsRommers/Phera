//! Included files share the caller symbol table, including dynamically created names.

use std::{cell::RefCell, rc::Rc};

use phera_bytecode::{CompiledUnit, Reg};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::{Vm, calls::frame::FrameView};

/// A callee register and its shared caller reference cell.
pub(crate) struct ScopeBinding {
    pub(crate) register: Reg,
    pub(crate) name: Option<String>,
    pub(crate) value: Option<Value>,
}

/// An included symbol can be absent after unset or contain a newly bound reference.
struct ScopeEntry {
    name: String,
    value: Option<Value>,
}

impl Vm {
    /// Included and evaluated code use the caller's variables and class context.
    pub(crate) fn enter_scoped_unit(
        &mut self,
        unit: std::sync::Arc<CompiledUnit>,
        destination: Option<Reg>,
    ) -> Result<(), NativeError> {
        use crate::calls::{frame::Frame, prepared_call::CallKind};

        if self.call_stack.depth() >= self.frame_limit {
            return Err(NativeError::error(format!(
                "Maximum call stack depth of {} exceeded",
                self.frame_limit
            )));
        }
        self.publish_declaration_warnings(&unit);
        self.declare_included_functions(&unit)?;
        let function = self.code.load(&unit);
        self.hoist_classes(&unit, function)?;
        let bindings = self.include_bindings(&unit)?;
        let context = self.current().and_then(|frame| frame.method());
        let kind = if let Some(context) = &context {
            self.code.set_declaring_class(function, context.scope.index);
            let receiver = context.receiver.clone().map(Value::Object);
            self.registers.push_undefined();
            let index = self.registers.len() - 1;
            self.registers.window_mut(index, 1)[0] = receiver;
            CallKind::Method
        } else {
            CallKind::Function
        };
        let base = self.registers.len();
        let mut count = unit.main.registers as usize;
        self.registers.grow_to(base + count);
        self.call_stack
            .push(Frame::of_kind(function, kind, base, destination));
        if let Some(context) = context {
            self.call_stack.current_extras_mut().called = Some(context.called.index);
        }
        for ScopeBinding {
            register,
            name,
            value,
        } in bindings
        {
            if let Some(name) = name {
                self.registers.push_undefined();
                count += 1;
                let extras = self.call_stack.current_extras_mut();
                extras.dynamic_count = Some(count);
                extras.dynamic.insert(name, register);
            }
            self.registers.window_mut(base + register.index(), 1)[0] = value;
        }
        Ok(())
    }

    /// Included locals and dynamic caller names must refer to the same cells before execution.
    pub(crate) fn include_bindings(
        &mut self,
        unit: &CompiledUnit,
    ) -> Result<Vec<ScopeBinding>, NativeError> {
        let names = self.include_names(unit);
        let static_count = unit.main.locals.len();
        let mut bindings = Vec::with_capacity(names.len());
        let global = self.in_global_scope();
        let register_count = usize::from(unit.main.registers) + names.len() - static_count;
        if register_count > usize::from(u16::MAX) + 1 {
            return Err(NativeError::error("Phera register limit exceeded"));
        }
        for (index, name) in names.into_iter().enumerate() {
            let destination = if index < static_count {
                index
            } else {
                usize::from(unit.main.registers) + index - static_count
            };
            let value = if global {
                self.take_request_global(&name)
            } else {
                self.share_include_local(&name)?
            };
            bindings.push(ScopeBinding {
                register: Reg(destination as u16),
                name: (index >= static_count).then_some(name),
                value,
            });
        }
        Ok(bindings)
    }

    fn share_include_local(&mut self, name: &str) -> Result<Option<Value>, NativeError> {
        let register = self.named_slot(name)?;
        let slot = self
            .current_slot_mut(register)
            .ok_or_else(|| internal_error!("include variable slot"))?;
        let reference = slot.take().map(|value| {
            if value.is_reference() {
                value
            } else {
                Value::Reference(Rc::new(RefCell::new(value)))
            }
        });
        *slot = reference.clone();
        Ok(reference)
    }

    /// Caller-only names remain accessible through variable variables, compact, and extract.
    fn include_names(&self, unit: &CompiledUnit) -> Vec<String> {
        let mut names: Vec<_> = unit
            .main
            .locals
            .iter()
            .filter_map(|name| unit.names.get(name.index()).cloned())
            .collect();
        if let Some(parent) = self.current() {
            for (name, _) in parent.named_registers() {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        names
    }

    /// New dynamic names, unset symbols, and rebindings survive the included frame's return.
    pub(crate) fn finish_include_scope(&mut self) -> Result<(), NativeError> {
        let Some(frame) = self.current() else {
            return Ok(());
        };
        if frame.function_id().is_some() {
            return Ok(());
        }
        if self.call_stack.depth() == 1 {
            self.park_global_registers();
            return Ok(());
        }
        let global = self.in_global_scope();
        let entries = scope_entries(frame, &self.registers);
        let frame = self
            .call_stack
            .pop()
            .ok_or_else(|| internal_error!("include return without frame"))?;
        self.registers.truncate(frame.start());
        for ScopeEntry { name, value } in entries {
            if global {
                self.store_request_global(name, value);
                continue;
            }
            let register = self.named_slot(&name)?;
            if let Some(slot) = self.current_slot_mut(register) {
                *slot = value;
            }
        }
        // Ordinary return still owns recycling and writing the result register.
        self.call_stack
            .push(frame.moved_to(self.registers.len() + frame.kind().prefix()));
        Ok(())
    }
}

/// Snapshot raw bindings before releasing the included frame, preserving reference identity.
fn scope_entries(
    frame: FrameView<'_>,
    registers: &crate::calls::registers::Registers,
) -> Vec<ScopeEntry> {
    frame
        .named_registers()
        .map(|(name, register)| ScopeEntry {
            name: name.clone(),
            value: frame
                .slots(registers)
                .get(register.index())
                .cloned()
                .flatten(),
        })
        .collect()
}
