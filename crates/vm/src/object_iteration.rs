//! Reference foreach over objects follows live property positions and shares each property's cell.

use crate::{Vm, calls::frame::IteratorState};
use phera_bytecode::{Reg, Target};
use phera_extension_api::NativeError;
use phera_value::{Object, Value};
use std::{cell::RefCell, rc::Rc};

impl Vm {
    /// Resolve visibility against raw keys while preserving insertion positions after deletions.
    pub(crate) fn next_object_reference(
        &mut self,
        state: &mut IteratorState,
        object: &Rc<Object>,
        key: Option<Reg>,
        destination: Reg,
        end: Target,
    ) -> Result<(), NativeError> {
        let properties = object.to_array();
        let mut position = if state.started {
            state
                .position
                .and_then(|position| properties.next_position(position))
        } else {
            properties.first_position()
        };
        while let Some(index) = position {
            let Some((stored_key, value)) = properties.entry_at(index) else {
                break;
            };
            let name = crate::path_update::property_name(&stored_key);
            if self
                .property_key(object, &name)
                .is_ok_and(|resolved| resolved == stored_key)
            {
                let reference = self.object_iteration_cell(object, &stored_key, value.clone())?;
                state.position = Some(index);
                state.started = true;
                if let Some(key) = key {
                    self.set(key, Value::from(name));
                }
                drop(properties);
                let previous = self
                    .current()
                    .and_then(|frame| frame.slots(&self.registers).get(destination.index()))
                    .cloned()
                    .flatten();
                self.set(destination, reference);
                if let Some(Value::Reference(reference)) = previous {
                    self.release_property_reference(reference);
                }
                return Ok(());
            }
            position = properties.next_position(index);
        }
        self.release_iterator_subject(state)?;
        self.jump(end)
    }

    pub(crate) fn object_iteration_cell(
        &mut self,
        object: &Rc<Object>,
        key: &phera_value::ArrayKey,
        value: Value,
    ) -> Result<Value, NativeError> {
        self.check_iteration_reference(object, key)?;
        let reference = match value {
            Value::Reference(cell) => cell,
            value => Rc::new(RefCell::new(value)),
        };
        crate::reference_holders::ReferenceHolders::track_object(
            &self.property_references,
            object,
            key,
            &reference,
        )?;
        let reference = Value::Reference(reference);
        object.set(key.clone(), reference.clone());
        Ok(reference)
    }

    /// Foreach must reject readonly aliases just like an explicit property reference.
    fn check_iteration_reference(
        &self,
        object: &Object,
        key: &phera_value::ArrayKey,
    ) -> Result<(), NativeError> {
        if let Some(class) = self.classes.get(object.class.name())
            && let Some(property) = class
                .defaults
                .iter()
                .find(|property| property.key == *key && property.is_readonly)
        {
            return Err(NativeError::error(format!(
                "Cannot acquire reference to readonly property {}::${}",
                property.declaring, property.name
            )));
        }
        Ok(())
    }
}
