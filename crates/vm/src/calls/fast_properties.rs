//! Property and chained property/element reads, writes and increments that the
//! interpreter loop makes directly via each class's flat [`PropertyTable`].
//!
//! ```php
//! class Point {
//!     public function __construct(private int $x) {}
//!     public function x(): int { return $this->x; }              // Fetch $this->x
//!     public function field(string $k) {
//!         return $this->rsm->fieldMappings[$k];                  // Chained Fetch
//!     }
//!     public function move(int $by): void {
//!         $this->x = $this->x + $by;                             // AssignPath $this->x
//!         $this->moves++;                                        // IncDecPath $this->moves
//!     }
//! }
//! ```
//!
//! Every linked [`Class`] owns an immutable flat [`PropertyTable`] indexed by
//! process-interned case-sensitive [`PropKey`], packing slot index, visibility,
//! hooks, readonly and type-mask bits into a 16-byte `Copy` descriptor.
//! Single-property and chained property/element paths run on the very first
//! execution, across monomorphic and polymorphic receivers, with zero
//! per-instruction heap caches, zero `RefCell` site borrows, zero string
//! mangling and zero `WindowContext` reconstruction. Anything needing magic
//! methods, hooks, lazy initialization, coercions, dynamic property creation or
//! diagnostics answers [`Step::Slow`] with nothing changed.

use std::rc::Rc;

use phera_bytecode::{IncDec, NameId, PathBase, PathId, Reg, Step as PathStep};
use phera_value::{Object, Value};

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::calls::prepared_call::CallKind;
use crate::classes::property_table::PackedProp;
use crate::classes::{Class, ClassIndex};
use crate::fast::{ScalarStep, store};

/// Maximum path depth walked directly in the loop.
const MAX_CHAIN_STEPS: usize = 6;

impl Machine<'_> {
    /// Class scope index of the running frame without building a `WindowContext`.
    #[inline(always)]
    pub(super) fn caller_scope_index(&self, position: Position) -> Option<ClassIndex> {
        match position.frame.kind() {
            CallKind::Function => None,
            CallKind::Method | CallKind::Constructor => {
                self.code.facts(position.frame.function).class
            }
            CallKind::Closure => self.running_scope().map(|scope| scope.index),
        }
    }

    /// Resolve a declared property on `object` directly from its flat [`PropertyTable`].
    #[inline(always)]
    pub(super) fn resolve_prop<'a>(
        &self,
        position: Position,
        object: &'a Object,
        name: NameId,
        scope: Option<ClassIndex>,
    ) -> Option<(
        &'a crate::classes::property_table::PropertyTable,
        PackedProp,
    )> {
        if self.classes.has_lazy_objects && object.lazy().is_some() {
            return None;
        }
        let class = object.class.as_any().downcast_ref::<Class>()?;
        let table = class.property_table();
        if table.has_native_props() {
            return None;
        }
        let key = self.code.unit_prop_key(position.frame.function, name);
        let packed = table.resolve(key, scope)?;
        if !packed.visible_to(scope, self.classes) {
            return None;
        }
        Some((table, packed))
    }

    /// Read a single declared property slot directly from `object`.
    #[inline(always)]
    fn read_prop_value(
        &self,
        position: Position,
        object: &Object,
        name: NameId,
        scope: Option<ClassIndex>,
        quiet: bool,
    ) -> Option<Value> {
        let (table, packed) = self.resolve_prop(position, object, name, scope)?;
        if !packed.is_plain_readable() {
            return None;
        }
        let slot = packed.slot();
        let found = object.with_properties(|properties| {
            properties.layout()?;
            Some(properties.slot(slot).map(|value| value.deref().clone()))
        })??;
        match found {
            Some(value) => Some(value),
            None if quiet && table.quiet_null_when_missing() => Some(Value::Null),
            None => None,
        }
    }

    /// Read a declared array property and walk trailing `Step::Index` steps in a
    /// single borrow of `object.properties` without cloning intermediate arrays
    /// or string keys.
    #[inline]
    fn read_prop_indexed(
        &self,
        position: Position,
        object: &Object,
        name: NameId,
        index_steps: &[PathStep],
        scope: Option<ClassIndex>,
        quiet: bool,
    ) -> Option<Value> {
        let (table, packed) = self.resolve_prop(position, object, name, scope)?;
        if !packed.is_plain_readable() {
            return None;
        }
        let slot = packed.slot();
        let quiet_null = quiet && table.quiet_null_when_missing();
        object
            .with_properties(|properties| {
                properties.layout()?;
                let Some(root) = properties.slot(slot) else {
                    return quiet_null.then_some(Value::Null);
                };
                self.walk_array_indices(position, root, index_steps, quiet)
            })
            .flatten()
    }

    /// Walk one or more `Step::Index` steps starting from a borrowed `&Value`.
    #[inline]
    fn walk_array_indices(
        &self,
        position: Position,
        root: &Value,
        steps: &[PathStep],
        quiet: bool,
    ) -> Option<Value> {
        let mut current = root;
        for step in steps {
            let PathStep::Index(register) = *step else {
                return None;
            };
            let key = self.array_key_ref(position, register)?;
            match current {
                Value::Array(array) => match key.get(array) {
                    Some(Value::Reference(_)) => return None,
                    Some(element) => current = element,
                    None if quiet => return Some(Value::Null),
                    None => return None,
                },
                Value::Null if quiet => return Some(Value::Null),
                _ => return None,
            }
        }
        if current.is_reference() {
            return None;
        }
        Some(current.clone())
    }

    /// Read a single-property or multi-step chained property/index path directly
    /// in the interpreter loop (e.g. `$this->x`, `$other->x`, `$this->a[$k]`,
    /// `$this->_rsm->fieldMappings[$k]`, `$class->fieldMappings[$f]->type`,
    /// `$rows[$k]->prop`).
    pub(super) fn read_path_fast(
        &self,
        position: Position,
        path: PathId,
        quiet: bool,
    ) -> Option<Value> {
        let body = self.code.function(position.frame.function);
        let path_body = body.paths.get(path.index())?;
        let steps = path_body.steps.as_slice();
        if steps.is_empty() || steps.len() > MAX_CHAIN_STEPS {
            return None;
        }
        // Single-step property read: hottest path in framework code.
        if let [PathStep::Property(name)] = steps {
            let object = self.property_object(position, path_body.base)?;
            let scope = self.caller_scope_index(position);
            return self.read_prop_value(position, object, *name, scope, quiet);
        }
        // `[Property, Index..]` directly on `$this` or register: single borrow, zero array clone.
        if let [PathStep::Property(name), rest @ ..] = steps
            && rest.iter().all(|step| matches!(step, PathStep::Index(_)))
        {
            let object = self.property_object(position, path_body.base)?;
            let scope = self.caller_scope_index(position);
            return self.read_prop_indexed(position, object, *name, rest, scope, quiet);
        }
        // General chain with at least one Property step (e.g. `$this->a->b[$k]`,
        // `$class->fieldMappings[$f]->enumType`, `$rows[$k]->prop`).
        if !steps
            .iter()
            .any(|step| matches!(step, PathStep::Property(_)))
        {
            return None;
        }
        let scope = self.caller_scope_index(position);
        // Resolve step 0 directly from borrowed base without cloning `$this` or root array.
        let mut owned_current: Value = match (path_body.base, steps[0]) {
            (base, PathStep::Property(name)) => {
                let object = self.property_object(position, base)?;
                self.read_prop_value(position, object, name, scope, quiet)?
            }
            (PathBase::Register(root), PathStep::Index(index_reg)) => {
                let key = self.array_key_ref(position, index_reg)?;
                let base_slot = position.frame.base() + root.index();
                match self.registers.window(base_slot, 1) {
                    [Some(Value::Array(array))] => match key.get(array) {
                        Some(Value::Reference(_)) => return None,
                        Some(element) => element.clone(),
                        None if quiet => return Some(Value::Null),
                        None => return None,
                    },
                    [Some(Value::Null)] if quiet => return Some(Value::Null),
                    _ => return None,
                }
            }
            _ => return None,
        };
        let mut step_index = 1;

        while step_index < steps.len() {
            match steps[step_index] {
                PathStep::Property(name) => {
                    let Value::Object(object) = &owned_current else {
                        return if quiet && owned_current.is_null() {
                            Some(Value::Null)
                        } else {
                            None
                        };
                    };
                    let rest = &steps[step_index + 1..];
                    if !rest.is_empty()
                        && rest.iter().all(|step| matches!(step, PathStep::Index(_)))
                    {
                        return self.read_prop_indexed(position, object, name, rest, scope, quiet);
                    }
                    owned_current = self.read_prop_value(position, object, name, scope, quiet)?;
                    step_index += 1;
                }
                PathStep::Index(index_reg) => {
                    let key = self.array_key_ref(position, index_reg)?;
                    owned_current = match &owned_current {
                        Value::Array(array) => match key.get(array) {
                            Some(Value::Reference(_)) => return None,
                            Some(element) => element.clone(),
                            None if quiet => return Some(Value::Null),
                            None => return None,
                        },
                        Value::Null if quiet => return Some(Value::Null),
                        _ => return None,
                    };
                    step_index += 1;
                }
                _ => return None,
            }
        }
        Some(owned_current)
    }

    /// Walk prefix steps of a write/increment path to reach the target object.
    /// Objects are handles in PHP, so intermediate reads do not detach them.
    fn resolve_target_object(
        &self,
        position: Position,
        base: PathBase,
        prefix: &[PathStep],
        scope: Option<ClassIndex>,
    ) -> Option<Rc<Object>> {
        if prefix.is_empty() {
            return self.property_object(position, base).cloned();
        }
        if prefix.len() >= MAX_CHAIN_STEPS {
            return None;
        }
        let mut current: Value = match (base, prefix[0]) {
            (base, PathStep::Property(name)) => {
                let object = self.property_object(position, base)?;
                self.read_prop_value(position, object, name, scope, false)?
            }
            (PathBase::Register(root), PathStep::Index(index_reg)) => {
                let key = self.array_key_ref(position, index_reg)?;
                match self
                    .registers
                    .window(position.frame.base() + root.index(), 1)
                {
                    [Some(Value::Array(array))] => match key.get(array) {
                        Some(Value::Reference(_)) | None => return None,
                        Some(element) => element.clone(),
                    },
                    _ => return None,
                }
            }
            _ => return None,
        };
        for &step in &prefix[1..] {
            current = match step {
                PathStep::Property(name) => {
                    let Value::Object(object) = &current else {
                        return None;
                    };
                    self.read_prop_value(position, object, name, scope, false)?
                }
                PathStep::Index(index_reg) => {
                    let key = self.array_key_ref(position, index_reg)?;
                    let Value::Array(array) = &current else {
                        return None;
                    };
                    match key.get(array) {
                        Some(Value::Reference(_)) | None => return None,
                        Some(element) => element.clone(),
                    }
                }
                _ => return None,
            };
        }
        match current {
            Value::Object(object) => Some(object),
            _ => None,
        }
    }

    /// `Fetch` in read mode of a property or chained property/index path.
    #[inline(never)]
    pub(super) fn fetch_property<const GUARDED: bool>(
        &mut self,
        position: Position,
        destination: Reg,
        path: PathId,
    ) -> Step {
        let Some(value) = self.read_path_fast(position, path, false) else {
            return Step::Slow;
        };
        let slots = self.registers.tail_mut(position.frame.base());
        match store::<GUARDED, _>(slots, destination, value) {
            ScalarStep::Complete | ScalarStep::Released => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }

    /// `Fetch` of a property or chained property/index path in a quiet mode
    /// (`??`, `isset`, `empty`).
    #[inline(never)]
    pub(super) fn fetch_property_quietly<const GUARDED: bool>(
        &mut self,
        position: Position,
        destination: Reg,
        path: PathId,
    ) -> Step {
        let Some(value) = self.read_path_fast(position, path, true) else {
            return Step::Slow;
        };
        let slots = self.registers.tail_mut(position.frame.base());
        match store::<GUARDED, _>(slots, destination, value) {
            ScalarStep::Complete | ScalarStep::Released => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }

    /// `AssignPath`, or with `result` `AssignPathFromLocal`, of a value to
    /// a declared property (single or chained: `$this->x = $v`, `$this->a->b = $v`).
    #[inline(never)]
    pub(super) fn assign_property<const GUARDED: bool>(
        &mut self,
        position: Position,
        path: PathId,
        (source, result): (Reg, Option<Reg>),
    ) -> Step {
        let base = position.frame.base();
        // Once the property is written, the value must be stored.
        if result.is_some_and(|result| self.registers.window(base + result.index(), 1).is_empty()) {
            return Step::Slow;
        }
        let value = match self.registers.window(base + source.index(), 1) {
            [Some(value)] => value.deref().clone(),
            _ => return Step::Slow,
        };
        if value.is_reference() {
            return Step::Slow;
        }
        let body = self.code.function(position.frame.function);
        let Some(path_body) = body.paths.get(path.index()) else {
            return Step::Slow;
        };
        let Some((PathStep::Property(last_name), prefix)) = path_body.steps.split_last() else {
            return Step::Slow;
        };
        let scope = self.caller_scope_index(position);
        let written = if prefix.is_empty() {
            let Some(object) = self.property_object(position, path_body.base) else {
                return Step::Slow;
            };
            self.write_declared_prop(position, object, *last_name, scope, &value)
        } else {
            let Some(object) = self.resolve_target_object(position, path_body.base, prefix, scope)
            else {
                return Step::Slow;
            };
            self.write_declared_prop(position, &object, *last_name, scope, &value)
        };
        if !written {
            return Step::Slow;
        }
        if let Some(result) = result {
            let _ = store::<GUARDED, _>(self.registers.tail_mut(base), result, value);
        }
        Step::Done(position.ip)
    }

    /// Write a declared property via [`PropertyTable`] (including class-typed
    /// object assignments, for both single and chained targets, with zero site cache).
    #[inline(always)]
    pub(super) fn write_declared_prop(
        &self,
        position: Position,
        object: &Object,
        name: NameId,
        scope: Option<ClassIndex>,
        value: &Value,
    ) -> bool {
        let Some((table, packed)) = self.resolve_prop(position, object, name, scope) else {
            return false;
        };
        if !packed.is_plain_writable() {
            return false;
        }
        if packed.allows_type(crate::types::own_type(value)) {
            return write_slot_direct(object, packed.slot(), value);
        }
        if packed.has_class_type()
            && let Value::Object(val_obj) = value
            && let Some(val_class) = val_obj.class.as_any().downcast_ref::<Class>()
            && table.allows_object_class(packed.slot(), val_class, self.classes)
        {
            return write_slot_direct(object, packed.slot(), value);
        }
        false
    }

    /// Write an element of an array held in a declared property (single or chained:
    /// `$this->items[$k] = $v`, `$this->a->items[$k] = $v`, `$this->a->items[] = $v`).
    pub(super) fn write_property_element_fast(
        &self,
        position: Position,
        base: PathBase,
        steps: &[PathStep],
        value: Value,
    ) -> Option<Option<Value>> {
        // Find the last `Step::Property` followed exclusively by `Index`/`Append` steps.
        let prop_pos = steps
            .iter()
            .rposition(|step| matches!(step, PathStep::Property(_)))?;
        let PathStep::Property(prop_name) = steps[prop_pos] else {
            return None;
        };
        let tail = &steps[prop_pos + 1..];
        if tail.is_empty() {
            return None;
        }
        let keys = self.write_keys(position, tail)?;
        let scope = self.caller_scope_index(position);
        let prefix = &steps[..prop_pos];
        let owned_target;
        let target: &Object = if prefix.is_empty() {
            self.property_object(position, base)?
        } else {
            owned_target = self.resolve_target_object(position, base, prefix, scope)?;
            &owned_target
        };
        let (_, packed) = self.resolve_prop(position, target, prop_name, scope)?;
        if !packed.is_plain_writable() || !packed.allows_array() {
            return None;
        }
        let slot = packed.slot();
        target
            .update(|properties| {
                properties.layout()?;
                let Some(Value::Array(array)) = properties.slot_mut(slot) else {
                    return None;
                };
                if !crate::element_writes::writable(array, keys.as_slice(), keys.target) {
                    return None;
                }
                crate::element_writes::write(array, keys.as_slice(), keys.target, value)
            })
            .flatten()
    }

    /// `IncDecPath` of one declared integer property (single or chained).
    #[inline(never)]
    pub(super) fn increment_property<const GUARDED: bool>(
        &mut self,
        position: Position,
        (mode, destination): (IncDec, Reg),
        path: PathId,
    ) -> Step {
        let slot_reg = position.frame.base() + destination.index();
        match self.registers.window(slot_reg, 1) {
            [_] => {}
            _ => return Step::Slow,
        }
        let body = self.code.function(position.frame.function);
        let Some(path_body) = body.paths.get(path.index()) else {
            return Step::Slow;
        };
        let Some((PathStep::Property(last_name), prefix)) = path_body.steps.split_last() else {
            return Step::Slow;
        };
        let scope = self.caller_scope_index(position);
        let owned_target;
        let target: &Object = if prefix.is_empty() {
            let Some(object) = self.property_object(position, path_body.base) else {
                return Step::Slow;
            };
            object
        } else {
            let Some(object) = self.resolve_target_object(position, path_body.base, prefix, scope)
            else {
                return Step::Slow;
            };
            owned_target = object;
            &owned_target
        };
        let Some((_, packed)) = self.resolve_prop(position, target, *last_name, scope) else {
            return Step::Slow;
        };
        if !packed.is_plain_writable() || !packed.allows_int() {
            return Step::Slow;
        }
        let prop_slot = packed.slot();
        let Some(value) = target
            .update(|properties| {
                properties.layout()?;
                let Some(Value::Int(slot)) = properties.slot_mut(prop_slot) else {
                    return None;
                };
                let old = *slot;
                let delta = if matches!(mode, IncDec::PreIncrement | IncDec::PostIncrement) {
                    1
                } else {
                    -1
                };
                let new = old.checked_add(delta)?;
                *slot = new;
                Some(Value::Int(
                    if matches!(mode, IncDec::PostIncrement | IncDec::PostDecrement) {
                        old
                    } else {
                        new
                    },
                ))
            })
            .flatten()
        else {
            return Step::Slow;
        };
        let slots = self.registers.tail_mut(position.frame.base());
        match store::<GUARDED, _>(slots, destination, value) {
            ScalarStep::Complete | ScalarStep::Released => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }

    /// The object a property path starts from.
    #[inline(always)]
    pub(super) fn property_object(
        &self,
        position: Position,
        base: PathBase,
    ) -> Option<&Rc<Object>> {
        match base {
            PathBase::This { .. } => {
                if position.frame.kind() == CallKind::Function {
                    return None;
                }
                match self.registers.window(position.frame.base() - 1, 1) {
                    [Some(Value::Object(object))] => Some(object),
                    _ => None,
                }
            }
            PathBase::Register(register) => {
                match self
                    .registers
                    .window(position.frame.base() + register.index(), 1)
                {
                    [Some(Value::Object(object))] => Some(object),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

#[inline(always)]
pub(super) fn write_slot_direct(object: &Object, slot: usize, value: &Value) -> bool {
    object
        .update(|properties| {
            properties.layout()?;
            match properties.slot_mut(slot) {
                Some(Value::Reference(_)) => Some(false),
                Some(destination) => {
                    *destination = value.clone();
                    Some(true)
                }
                None => {
                    properties.set_slot(slot, value.clone());
                    Some(true)
                }
            }
        })
        .flatten()
        .unwrap_or(false)
}
