//! Included files share the caller symbol table, including dynamically created names.

use std::{cell::RefCell, rc::Rc};

use phera_bytecode::{CompiledUnit, Reg};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::{Vm, calls::frame::Frame};

/// A callee register and its shared caller reference cell.
pub(crate) struct ScopeBinding {
    pub(crate) register: Reg,
    pub(crate) name: Option<String>,
    pub(crate) value: Value,
}

/// An included symbol can be absent after unset or contain a newly bound reference.
struct ScopeEntry {
    name: String,
    value: Option<Value>,
}

impl Vm {
    /// Included locals and dynamic caller names must refer to the same cells before execution.
    pub(crate) fn include_bindings(
        &mut self,
        unit: &CompiledUnit,
    ) -> Result<Vec<ScopeBinding>, NativeError> {
        let names = self.include_names(unit);
        let static_count = unit.main.locals.len();
        let mut bindings = Vec::with_capacity(names.len());
        for (index, name) in names.into_iter().enumerate() {
            let register = self.named_slot(&name)?;
            let parent = self
                .call_stack
                .current_mut()
                .ok_or_else(|| internal_error!("include without parent"))?;
            let slot = parent
                .slots_mut(&mut self.registers)
                .get_mut(register.index())
                .ok_or_else(|| internal_error!("include variable slot"))?;
            let value = slot.take().unwrap_or_default();
            let reference = if value.is_reference() {
                value
            } else {
                Value::Reference(Rc::new(RefCell::new(value)))
            };
            *slot = Some(reference.clone());
            let destination = if index < static_count {
                index
            } else {
                unit.main.registers as usize + index - static_count
            };
            bindings.push(ScopeBinding {
                register: Reg(u16::try_from(destination)
                    .map_err(|_| NativeError::error("Phera register limit exceeded"))?),
                name: (index >= static_count).then_some(name),
                value: reference,
            });
        }
        Ok(bindings)
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
            let parent_names = parent
                .function(&self.code)
                .locals
                .iter()
                .filter_map(|name| parent.unit(&self.code).names.get(name.index()))
                .chain(
                    self.call_stack.extras[parent.depth]
                        .dynamic
                        .iter()
                        .map(|(name, _)| name),
                );
            for name in parent_names {
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
        if frame.function_id(&self.code).is_some() || frame.destination.is_none() {
            return Ok(());
        }
        let entries = scope_entries(self, frame, &self.registers);
        let mut frame = self
            .call_stack
            .pop()
            .ok_or_else(|| internal_error!("include return without frame"))?;
        self.registers.truncate(frame.base);
        for ScopeEntry { name, value } in entries {
            let register = self.named_slot(&name)?;
            let parent = self
                .call_stack
                .current_mut()
                .ok_or_else(|| internal_error!("include return without parent"))?;
            if let Some(slot) = parent
                .slots_mut(&mut self.registers)
                .get_mut(register.index())
            {
                *slot = value;
            }
        }
        // Ordinary return still owns recycling and writing the result register.
        frame.base = self.registers.len();
        self.call_stack.push(frame);
        Ok(())
    }
}

/// Snapshot raw bindings before releasing the included frame, preserving reference identity.
fn scope_entries(
    vm: &Vm,
    frame: &Frame,
    registers: &crate::calls::registers::Registers,
) -> Vec<ScopeEntry> {
    frame
        .function(&vm.code)
        .locals
        .iter()
        .enumerate()
        .filter_map(|(index, name)| {
            frame
                .unit(&vm.code)
                .names
                .get(name.index())
                .map(|name| (name, Reg(index as u16)))
        })
        .chain(
            vm.call_stack.extras[frame.depth]
                .dynamic
                .iter()
                .map(|(name, register)| (name, *register)),
        )
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
