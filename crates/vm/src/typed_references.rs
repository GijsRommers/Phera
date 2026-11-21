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

use crate::{
    AccessPath, Vm,
    access_mode::{Binding, Creation, Missing},
    classes::class::PropertyDefault,
    inline_stack::InlineStack,
    path_update::PathKey,
    types::TypeChecking,
};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::Value;
use std::{cell::RefCell, rc::Rc};

impl Vm {
    /// Binding an untyped cell may convert it. An already typed cell keeps its value.
    pub(crate) fn coerce_property_binding(
        &mut self,
        reference: &Rc<RefCell<Value>>,
        property: &PropertyDefault,
        ty: &phera_bytecode::Type,
    ) -> Result<(), NativeError> {
        let value = reference
            .try_borrow()
            .map_err(|_| internal_error!("borrowed property alias"))?
            .clone();
        let converted = self.coerce_property(value.clone(), property, ty)?;
        if std::mem::discriminant(&value) == std::mem::discriminant(&converted) {
            return Ok(());
        }
        let holders = self.property_references.borrow();
        if let Some(holder) = holders
            .constraints
            .get(&(Rc::as_ptr(reference) as usize))
            .and_then(|properties| properties.values().next())
        {
            return Err(NativeError::type_error(format!(
                "Reference with value of type {} held by property {} is not compatible with property {}",
                crate::types::property_value_type(&value),
                reference_property_name(holder),
                reference_property_name(property),
            )));
        }
        drop(holders);
        *reference
            .try_borrow_mut()
            .map_err(|_| internal_error!("borrowed property alias"))? = converted;
        Ok(())
    }

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
                properties.values().any(|property| {
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
        value: Value,
    ) -> NativeResult {
        let index = self.property_references.borrow();
        if index.constraints.is_empty() {
            return Ok(value);
        }
        let Some(holders) = index.constraints.get(&(Rc::as_ptr(reference) as usize)) else {
            return Ok(value);
        };
        let mut constraints = crate::inline_stack::InlineStack::new();
        for property in holders.values() {
            constraints.push(property.clone());
        }
        drop(index);
        let mut holders = constraints.iter();
        let Some(first) = holders.next() else {
            return Ok(value);
        };
        if constraints.len() == 1 {
            return self.coerce_reference_property(value, first);
        }
        // All holders must choose the same conversion from the original
        // input. Scalar conversions here are deterministic for a result type.
        let converted = self.coerce_reference_property(value.clone(), first)?;
        for property in holders {
            let candidate = self.coerce_reference_property(value.clone(), property)?;
            if std::mem::discriminant(&converted) != std::mem::discriminant(&candidate) {
                return Err(conflicting_reference_types(&value, first, property));
            }
        }
        Ok(converted)
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
        let actual = crate::types::property_value_type(&value);
        self.coerce_property(value, property, declaration)
            .map_err(|_| reference_type_error(&actual, property))
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

    /// Acquiring a PHP alias initializes a nullable static slot to null.
    pub(crate) fn acquire_static_property_reference(
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
        let storage = self
            .class_states
            .static_property(property)
            .ok_or_else(|| internal_error!("static storage"))?;
        if !storage.value_initialized.get() {
            if property
                .ty
                .as_ref()
                .is_some_and(|ty| !ty.mask.intersects(phera_bytecode::TypeMask::NULL))
            {
                return Err(uninitialized_reference_error(property));
            }
            storage.value_initialized.set(true);
        }
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
        let storage = self
            .class_states
            .static_property(property)
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
        match path_reference(root, keys) {
            Some(reference) => self.check_reference_assignment(&reference, value),
            None => Ok(value),
        }
    }

    /// Only a weak assignment of a Stringable object can run PHP during a reference write.
    pub(crate) fn may_cast_typed_reference(&self, value: &Value) -> bool {
        matches!(value, Value::Object(_))
            && !self.property_references.borrow().constraints.is_empty()
            && self
                .current()
                .is_some_and(|frame| !frame.function().strict_types)
            && Self::has_string_conversion(value)
    }

    pub(crate) async fn assign_indexed_stringable(
        &mut self,
        place: &AccessPath,
        value: Value,
    ) -> Result<Option<Value>, NativeError> {
        let register = self
            .base_register(place, Creation::Create, Missing::Silent)?
            .ok_or_else(|| internal_error!("missing place"))?;
        let keys = self.keys(place)?;
        if let Some(reference) = path_reference(&self.raw_local(register), &keys)
            && let Some(converted) = self.assign_stringable_reference(&reference, &value).await?
        {
            return Ok(Some(converted));
        }
        self.write_resolved_place(place, register, keys, value, Binding::ByValue)?;
        Ok(None)
    }

    pub(crate) async fn assign_stringable_reference(
        &mut self,
        reference: &Rc<RefCell<Value>>,
        value: &Value,
    ) -> Result<Option<Value>, NativeError> {
        // Hold the selected cell across PHP code, with no borrow on the path or type index.
        let constraints: Vec<_> = {
            let index = self.property_references.borrow();
            let Some(properties) = index.constraints.get(&(Rc::as_ptr(reference) as usize)) else {
                return Ok(None);
            };
            properties
                .values()
                .filter_map(|property| {
                    property
                        .ty
                        .as_ref()
                        .map(|ty| (property.clone(), ty.clone()))
                })
                .collect()
        };
        let needs_string = |vm: &Vm, ty| vm.needs_string_coercion(value, ty, TypeChecking::Coerce);
        if !constraints.iter().any(|(_, ty)| needs_string(self, ty)) {
            return Ok(None);
        }

        let mut first: Option<(&PropertyDefault, Value)> = None;
        for (property, ty) in &constraints {
            let candidate = if needs_string(self, ty) {
                if let Some((first, Value::Object(_))) = first.as_ref() {
                    return Err(conflicting_reference_types(value, first, property));
                }
                match self.object_to_string(value).await {
                    Ok(Some(string)) => Value::String(string),
                    Ok(None) => return Err(reference_type_error(&value.type_name(), property)),
                    Err(error) => {
                        return Err(self.wrap_reference_cast_error(error, value, property));
                    }
                }
            } else {
                self.coerce_reference_property(value.clone(), property)?
            };
            if let Some((first, result)) = &first {
                if phera_value::strict_equals(result, &candidate, self)? != Value::Bool(true) {
                    return Err(conflicting_reference_types(value, first, property));
                }
            } else {
                first = Some((property, candidate));
            }
        }
        let Some((_, converted)) = first else {
            return Ok(None);
        };
        *reference
            .try_borrow_mut()
            .map_err(|_| internal_error!("borrowed typed array alias"))? = converted.clone();
        Ok(Some(converted))
    }

    fn wrap_reference_cast_error(
        &mut self,
        error: NativeError,
        value: &Value,
        property: &PropertyDefault,
    ) -> NativeError {
        use crate::exceptions::{Thrown, throwable_root};
        let previous = self.materialize_error(error);
        let NativeError::Unwind(state) = &previous else {
            return previous;
        };
        let Some(thrown) = state.0.downcast_ref::<Thrown>() else {
            return previous;
        };
        let error = self.materialize_error(reference_type_error(&value.type_name(), property));
        if let NativeError::Unwind(state) = &error
            && let Some(assignment) = state.0.downcast_ref::<Thrown>()
        {
            assignment.object.set(
                phera_value::Object::private_key(throwable_root(&assignment.object), "previous"),
                Value::Object(thrown.object.clone()),
            );
        }
        error
    }
}

pub(crate) fn path_reference(
    root: &Value,
    keys: &InlineStack<PathKey>,
) -> Option<Rc<RefCell<Value>>> {
    let mut target = root.clone();
    for key in keys.iter() {
        let next = match (&*target.deref(), key) {
            (Value::Object(object), PathKey::Property(key)) => {
                crate::property_storage::read(object, key)?
            }
            (Value::Array(array), PathKey::Index(Some(key))) => array.get(key)?.clone(),
            _ => return None,
        };
        target = next;
    }
    match target {
        Value::Reference(reference) => Some(reference),
        _ => None,
    }
}

fn reference_type_error(actual: &str, property: &PropertyDefault) -> NativeError {
    NativeError::type_error(format!(
        "Cannot assign {} to reference held by property {}",
        actual,
        reference_property_name(property)
    ))
}

pub(crate) fn uninitialized_reference_error(property: &PropertyDefault) -> NativeError {
    NativeError::error(format!(
        "Cannot access uninitialized non-nullable property {}::${} by reference",
        phera_value::class_display_name(&property.declaring),
        property.name,
    ))
}

/// A write through an alias must respect every holder before creating an array.
pub(crate) fn check_reference_array_promotion(
    reference: &Rc<RefCell<Value>>,
    references: &RefCell<crate::reference_holders::ReferenceHolders>,
) -> Result<(), NativeError> {
    let references = references.borrow();
    if let Some(properties) = references
        .constraints
        .get(&(Rc::as_ptr(reference) as usize))
        && let Some(property) = properties.values().find(|property| {
            property
                .ty
                .as_ref()
                .is_some_and(|ty| !crate::path_dimensions::accepts_promoted_array(ty))
        })
    {
        return Err(NativeError::type_error(format!(
            "Cannot auto-initialize an array inside a reference held by property {}",
            reference_property_name(property)
        )));
    }
    Ok(())
}

fn conflicting_reference_types(
    value: &Value,
    first: &PropertyDefault,
    other: &PropertyDefault,
) -> NativeError {
    NativeError::type_error(format!(
        "Cannot assign {} to reference held by property {} and property {}, as this would result in an inconsistent type conversion",
        crate::types::property_value_type(value),
        reference_property_name(first),
        reference_property_name(other)
    ))
}

pub(crate) fn reference_property_name(property: &PropertyDefault) -> String {
    let declaration = property
        .ty
        .as_ref()
        .map_or("mixed".into(), crate::types::php_type_display);
    format!(
        "{}::${} of type {}",
        phera_value::class_display_name(&property.declaring),
        property.name,
        declaration
    )
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
