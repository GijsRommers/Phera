//! PHP checks every typed property holding a reference when writing through any alias.
//!
//! ```php
//! class C { public int $n = 0; }
//! $c = new C; $r =& $c->n; $r = "abc";
//! // TypeError: Cannot assign string to reference held by property C::$n of type int
//! ```
//!
//! PHP stores the declarations on the reference itself. Our value representation stays an
//! Rc<RefCell<Value>>; a VM side table indexed by that cell's identity mirrors PHP's list.
//! Slot-owned leases add and remove declarations on binding, rebinding, unset and object drop.
//! Weak cell handles reserve allocation identities without retaining values. An empty typed
//! table makes unrelated reference writes return immediately, regardless of live object count.

use crate::{Vm, classes::class::PropertyDefault};
use phera_extension_api::NativeResult;
use phera_value::Value;
use std::{cell::RefCell, rc::Rc};

impl Vm {
    /// Only an object assigned through a typed reference can need a callback.
    pub(crate) fn local_reference_needs_string(
        &self,
        local: phera_bytecode::Reg,
        value: &Value,
    ) -> bool {
        let Some(frame) = self
            .current()
            .filter(|frame| !frame.function().strict_types)
        else {
            return false;
        };
        let Some(Some(Value::Reference(reference))) =
            frame.slots(&self.registers).get(local.index())
        else {
            return false;
        };
        self.property_references
            .borrow()
            .constraints
            .get(&(Rc::as_ptr(reference) as usize))
            .is_some_and(|properties| {
                properties.iter().any(|(_, property)| {
                    property.ty.as_ref().is_some_and(|ty| {
                        self.needs_string_coercion(value, ty, crate::types::TypeChecking::Coerce)
                    })
                })
            })
    }

    /// Look up only this cell; neither objects nor class declarations are scanned.
    #[inline(never)]
    pub(crate) fn check_reference_assignment(
        &mut self,
        reference: &Rc<RefCell<Value>>,
        mut value: Value,
    ) -> NativeResult {
        let index = self.property_references.borrow();
        if index.constraints.is_empty() {
            return Ok(value);
        }
        let Some(holders) = index.constraints.get(&(Rc::as_ptr(reference) as usize)) else {
            return Ok(value);
        };
        let mut constraints = crate::inline_stack::InlineStack::new();
        for (_, property) in holders {
            constraints.push(property.clone());
        }
        drop(index);
        for property in constraints.iter() {
            value = self.coerce_reference_property(value, property)?;
        }
        Ok(value)
    }

    /// The alias diagnostic names the property holding the type constraint, not the variable.
    fn coerce_reference_property(
        &mut self,
        value: Value,
        property: &PropertyDefault,
    ) -> NativeResult {
        let Some(declaration) = &property.ty else {
            return Ok(value);
        };
        let actual = match &value {
            Value::Object(object) => object.class.name().to_owned(),
            _ => value.type_name(),
        };
        self.coerce_property(value, property, declaration)
            .map_err(|_| {
                phera_extension_api::NativeError::type_error(format!(
                    "Cannot assign {actual} to reference held by property {}::${} of type {}",
                    property.declaring, property.name, declaration.display
                ))
            })
    }
}

impl Vm {
    /// A cell with no external aliases can collapse back into its owning property.
    pub(crate) fn release_property_reference(&self, reference: Rc<RefCell<Value>>) {
        if Rc::strong_count(&reference) != 2 {
            return;
        }
        let holder = self
            .property_references
            .borrow()
            .sole_object_holder(&reference);
        if let Some((object, key)) = holder
            && let Ok(value) = reference.try_borrow()
        {
            crate::property_storage::write(&object, key.clone(), value.clone());
            crate::reference_holders::ReferenceHolders::untrack_object(&object, &key);
        }
    }
}

impl Vm {
    /// Bulk property-table copies happen at clone and native object construction only.
    pub(crate) fn track_object_property_references(
        &self,
        object: &Rc<phera_value::Object>,
    ) -> Result<(), phera_extension_api::NativeError> {
        for (key, value) in object.to_array().iter() {
            if let Value::Reference(reference) = value {
                crate::reference_holders::ReferenceHolders::track_object(
                    &self.property_references,
                    object,
                    &key,
                    reference,
                )?;
            }
        }
        Ok(())
    }

    /// Static reads use internal cells too; index them only when PHP acquires an alias.
    pub(crate) fn track_static_property_reference(
        &mut self,
        class: phera_bytecode::Reg,
        name: phera_bytecode::Reg,
    ) -> Result<(), phera_extension_api::NativeError> {
        let (class_name, name) = self.static_property_names(class, name)?;
        let class = self
            .classes
            .get(&class_name)
            .ok_or_else(|| internal_error!("static class"))?;
        let property = class
            .static_properties
            .iter()
            .rev()
            .find(|property| property.name == name)
            .ok_or_else(|| internal_error!("static property"))?;
        let storage = property
            .static_storage
            .as_ref()
            .ok_or_else(|| internal_error!("static storage"))?;
        storage.track(&self.property_references, property);
        Ok(())
    }

    /// Rebinding replaces the shared static slot, leaving the previous alias unconstrained.
    pub(crate) fn bind_static_property_reference(
        &mut self,
        class: phera_bytecode::Reg,
        name: phera_bytecode::Reg,
        value: Value,
    ) -> Result<(), phera_extension_api::NativeError> {
        let (class_name, name) = self.static_property_names(class, name)?;
        let class = self
            .classes
            .get(&class_name)
            .ok_or_else(|| internal_error!("static class"))?;
        let property = class
            .static_properties
            .iter()
            .rev()
            .find(|property| property.name == name)
            .ok_or_else(|| internal_error!("static property"))?;
        let storage = property
            .static_storage
            .as_ref()
            .ok_or_else(|| internal_error!("static storage"))?;
        let reference = match value {
            Value::Reference(reference) => reference,
            value => Rc::new(RefCell::new(value)),
        };
        storage.rebind(reference, &self.property_references, property);
        Ok(())
    }
}

impl Vm {
    /// Array elements and nested paths can hold the same constrained cell as a local alias.
    pub(crate) fn check_path_reference_assignment(
        &mut self,
        root: &Value,
        keys: &crate::inline_stack::InlineStack<crate::path_update::PathKey>,
        value: Value,
    ) -> NativeResult {
        if self.property_references.borrow().constraints.is_empty() {
            return Ok(value);
        }
        let mut target = root.clone();
        for key in keys.iter() {
            let next = match (&*target.deref(), key) {
                (Value::Object(object), crate::path_update::PathKey::Property(key)) => {
                    crate::property_storage::read(object, key).unwrap_or_default()
                }
                (Value::Array(array), crate::path_update::PathKey::Index(Some(key))) => {
                    array.get(key).cloned().unwrap_or_default()
                }
                _ => return Ok(value),
            };
            target = next;
        }
        if let Value::Reference(reference) = target {
            self.check_reference_assignment(&reference, value)
        } else {
            Ok(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        hint::black_box,
        rc::Rc,
        sync::Arc,
        time::{Duration, Instant},
    };

    use phera_extension_api::{Context, Registry};
    use phera_value::{ArrayKey, PhpArray, Value};

    use crate::{Vm, reference_holders::ReferenceHolders};

    async fn interpreter() -> Vm {
        let unit = phera_compiler::compile(
            "<?php class Counter { public int $n = 0; }",
            "reference-index.php",
        )
        .unwrap();
        let mut interpreter = Vm::new(Arc::new(Registry::new()));
        assert_eq!(interpreter.run(Arc::new(unit)).await, 0);
        interpreter
    }

    fn million_writes(interpreter: &mut Vm, reference: &Rc<RefCell<Value>>) -> Duration {
        let started = Instant::now();
        for value in 0..1_000_000 {
            let value = interpreter
                .check_reference_assignment(black_box(reference), Value::Int(black_box(value)))
                .unwrap();
            *reference.borrow_mut() = value;
        }
        started.elapsed()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn native_payload_conflicts_are_explicit_and_leave_storage_unchanged() {
        let mut interpreter = interpreter().await;
        let key = ArrayKey::str("n");
        let mut properties = PhpArray::new();
        properties.insert(key.clone(), Value::Int(1));
        let Value::Object(object) = interpreter.create_object("Counter", properties).unwrap()
        else {
            panic!("expected object");
        };
        *object.native.borrow_mut() = Some(Box::new(()));
        let reference = Rc::new(RefCell::new(Value::Int(2)));
        assert!(
            ReferenceHolders::track_object(
                &interpreter.property_references,
                &object,
                &key,
                &reference,
            )
            .is_err()
        );
        assert_eq!(object.get(&key), Some(Value::Int(1)));
        assert!(object.native.borrow().as_ref().unwrap().is::<()>());
        assert!(interpreter.property_references.borrow().is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reference_writes_do_not_depend_on_live_object_count() {
        let mut interpreter = interpreter().await;
        let class = interpreter.classes.get("stdClass").unwrap();
        let objects: Vec<_> = (0..200_000)
            .map(|_| interpreter.classes.allocate(class.clone()))
            .collect();
        let reference = Rc::new(RefCell::new(Value::Int(0)));
        let unconstrained = million_writes(&mut interpreter, &reference);
        assert!(
            interpreter
                .property_references
                .borrow()
                .constraints
                .is_empty()
        );

        let class = interpreter.classes.get("Counter").unwrap();
        let object = interpreter.classes.allocate(class);
        let key = ArrayKey::str("n");
        object.set(key.clone(), Value::Reference(reference.clone()));
        ReferenceHolders::track_object(&interpreter.property_references, &object, &key, &reference)
            .unwrap();
        let constrained = million_writes(&mut interpreter, &reference);
        assert!(
            interpreter
                .check_reference_assignment(&reference, Value::Array(Rc::new(PhpArray::new())))
                .is_err()
        );
        assert_eq!(*reference.borrow(), Value::Int(999_999));
        let unrelated = Rc::new(RefCell::new(Value::Int(0)));
        let indexed_miss = million_writes(&mut interpreter, &unrelated);
        eprintln!(
            "200000 live objects, 1000000 writes: empty={unconstrained:?}, \
             typed={constrained:?}, unrelated={indexed_miss:?}"
        );
        if !cfg!(debug_assertions) {
            for elapsed in [unconstrained, constrained, indexed_miss] {
                assert!(
                    elapsed < Duration::from_secs(1),
                    "reference writes took {elapsed:?}"
                );
            }
        }
        drop(object);
        drop(objects);
        drop(reference);
        drop(unrelated);
        assert!(
            interpreter
                .property_references
                .borrow()
                .constraints
                .is_empty()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn dropping_indirect_owners_removes_every_reference_entry() {
        let mut interpreter = interpreter().await;
        let reference = Rc::new(RefCell::new(Value::Int(1)));
        let weak_reference = Rc::downgrade(&reference);
        let mut properties = PhpArray::new();
        properties.insert(ArrayKey::str("n"), Value::Reference(reference.clone()));
        let object = interpreter.create_object("Counter", properties).unwrap();
        let mut array = PhpArray::new();
        array.push(object).unwrap();
        let owner = Value::Array(Rc::new(array));
        assert_eq!(
            interpreter.property_references.borrow().constraints.len(),
            1
        );
        drop(owner);
        assert!(
            interpreter
                .property_references
                .borrow()
                .constraints
                .is_empty()
        );
        assert_eq!(
            interpreter
                .check_reference_assignment(&reference, Value::from("released"))
                .unwrap(),
            Value::from("released")
        );
        drop(reference);
        assert!(weak_reference.upgrade().is_none());
        assert!(interpreter.property_references.borrow().is_empty());
    }
}
