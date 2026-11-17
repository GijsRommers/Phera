//! Global paths resolve directly against the request symbol table, independent of local scope.

use phera_bytecode::Reg;
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, PhpArray, Value};

use crate::{
    Vm,
    access_mode::{Creation, Missing},
};

impl Vm {
    /// Find a global's shared cell without adding work to functions using only locals.
    pub(crate) fn global_register(
        &mut self,
        name: String,
        scratch: Reg,
        creation: Creation,
        missing: Missing,
    ) -> Result<Option<Reg>, NativeError> {
        let main = self
            .frame_at(0)
            .ok_or_else(|| internal_error!("global without main"))?;
        let index = main
            .function()
            .locals
            .iter()
            .position(|candidate| main.unit().names.get(candidate.index()) == Some(&name));
        let main_register = index
            .map(|index| Reg(index as u16))
            .or_else(|| main.extras.dynamic.get(&name).copied());
        let existing = main_register
            .and_then(|register| main.slots(&self.registers).get(register.index()))
            .and_then(Option::as_ref)
            .cloned()
            .or_else(|| {
                self.globals
                    .iter()
                    .find(|(candidate, _)| *candidate == &name)
                    .map(|(_, value)| value.clone())
            });
        if existing.is_none() && creation == Creation::Existing {
            if missing == Missing::Warn {
                self.warn(format!("Undefined global variable ${name}"));
            }
            return Ok(None);
        }
        if let Some(value) = existing {
            self.globals.insert(name.clone(), value);
        }
        let reference = self.globals.reference(name.clone());
        if let Some(main_register) = main_register {
            if let Some(slot) = self.main_slot_mut(main_register) {
                *slot = Some(reference.clone());
            }
            if self.call_stack.depth() == 1 {
                return Ok(Some(main_register));
            }
        }
        self.set(scratch, reference);
        Ok(Some(scratch))
    }

    /// Reading the whole table copies values rather than exposing its writable backing cells.
    pub(crate) fn global_snapshot(&self) -> NativeResult {
        let mut values = PhpArray::new();
        for (name, value) in self.globals.iter() {
            values.insert(ArrayKey::str(name.as_str()), self.copy_global_value(value));
        }
        if let Some(main) = self.frames().next() {
            for (index, name) in main.function().locals.iter().enumerate() {
                if let Some(name) = main.unit().names.get(name.index())
                    && let Some(Some(value)) = main.slots(&self.registers).get(index)
                {
                    values.insert(ArrayKey::str(name.as_str()), self.copy_global_value(value));
                }
            }
            for (name, register) in main.extras.dynamic.iter() {
                if let Some(Some(value)) = main.slots(&self.registers).get(register.index()) {
                    values.insert(ArrayKey::str(name.as_str()), self.copy_global_value(value));
                }
            }
        }
        Ok(Value::Array(std::rc::Rc::new(values)))
    }

    /// Copies preserve user aliases while discarding cells created only for VM global access.
    fn copy_global_value(&self, value: &Value) -> Value {
        let Value::Reference(reference) = value else {
            return value.clone();
        };
        let same_cell = |candidate: &Value| matches!(candidate, Value::Reference(candidate) if std::rc::Rc::ptr_eq(candidate, reference));
        let mut internal_owners = self
            .globals
            .iter()
            .filter(|(_, value)| same_cell(value))
            .count();
        if let Some(main) = self.frames().next() {
            let named_cell = main
                .slots(&self.registers)
                .iter()
                .take(main.function().locals.len())
                .flatten()
                .any(&same_cell)
                || main.extras.dynamic.iter().any(|(_, register)| {
                    main.slots(&self.registers)
                        .get(register.index())
                        .and_then(Option::as_ref)
                        .is_some_and(same_cell)
                });
            internal_owners += usize::from(named_cell);
        }
        for frame in self.frames() {
            for (index, slot) in frame
                .slots(&self.registers)
                .iter()
                .enumerate()
                .skip(frame.function().locals.len())
            {
                if !frame
                    .extras
                    .dynamic
                    .iter()
                    .any(|(_, register)| register.index() == index)
                    && slot.as_ref().is_some_and(same_cell)
                {
                    internal_owners += 1;
                }
            }
        }
        if std::rc::Rc::strong_count(reference) > internal_owners {
            value.clone()
        } else {
            value.deref().clone()
        }
    }

    /// Rebinding a whole global replaces its symbol cell, including the main-scope slot.
    pub(crate) fn publish_global(&mut self, name: String, value: Value) {
        self.globals.insert(name.clone(), value.clone());
        let register = self.frame_at(0).and_then(|main| {
            main.function()
                .locals
                .iter()
                .position(|candidate| main.unit().names.get(candidate.index()) == Some(&name))
                .map(|index| Reg(index as u16))
                .or_else(|| main.extras.dynamic.get(&name).copied())
        });
        if let Some(slot) = register.and_then(|register| self.main_slot_mut(register)) {
            *slot = Some(value);
        }
    }

    /// Removing a symbol never writes null through other references to its former value.
    pub(crate) fn remove_global(&mut self, name: &str) {
        self.globals.remove(name);
        if let Some(main) = self.frame_at(0) {
            let index = main.function().locals.iter().position(|candidate| {
                main.unit()
                    .names
                    .get(candidate.index())
                    .is_some_and(|candidate| candidate == name)
            });
            let register = index
                .map(|index| Reg(index as u16))
                .or_else(|| main.extras.dynamic.get(name).copied());
            if let Some(register) = register {
                let (base, count) = (main.base(), main.count());
                if let Some(slot) = self
                    .registers
                    .window_mut(base, count)
                    .get_mut(register.index())
                {
                    *slot = None;
                }
            }
        }
    }
}
