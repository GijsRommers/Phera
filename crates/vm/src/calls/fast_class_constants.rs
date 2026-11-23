//! `Post::LIMIT`, read in the interpreter loop when the constant is ready.
//!
//! ```php
//! $type = ClassMetadata::GENERATOR_TYPE_NONE;  // FetchNamedClassConstant
//! ```
//!
//! This is the ready path of `Vm::fetch_named_class_constant`:
//! - the class the source names is found by its symbol's name key;
//! - the constant is public and not deprecated;
//! - the class is not an enum;
//! - its value is a native constant's value, or a PHP constant this
//!   request has already evaluated.
//!
//! Then the value is read through borrows and stored. Every other case
//! (the first read, which evaluates the initializer; a protected or private
//! constant; an enum case; a deprecation; an undeclared class) goes to the
//! VM's handler ([`Step::Slow`]).

use phera_bytecode::{ClassKind, NameId, PropKey, Reg, SymbolId};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::classes::Class;
use crate::classes::class::{ConstantInitializer, ConstantState};
use crate::fast::{ScalarStep, store};

impl Machine<'_> {
    /// `FetchNamedClassConstant`, when the constant is ready (including
    /// `self::`, `static::`, `parent::`, private/protected constants in scope,
    /// and validated backed enums).
    pub(super) fn fetch_named_class_constant<const GUARDED: bool>(
        &mut self,
        position: Position,
        destination: Reg,
        (class, name): (SymbolId, NameId),
    ) -> Step {
        let Some(value) = self.ready_named_class_constant(position, class, name) else {
            return Step::Slow;
        };
        let slots = self.registers.tail_mut(position.frame.base());
        match store::<GUARDED, _>(slots, destination, value) {
            ScalarStep::Complete | ScalarStep::Released => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }

    /// `FetchClassConstant` (`$object::CONST`, `$class::CONST`, `$object::class`).
    pub(super) fn fetch_class_constant<const GUARDED: bool>(
        &mut self,
        position: Position,
        destination: Reg,
        (class_reg, name): (Reg, NameId),
    ) -> Step {
        let Some(value) = self.ready_class_constant(position, class_reg, name) else {
            return Step::Slow;
        };
        let slots = self.registers.tail_mut(position.frame.base());
        match store::<GUARDED, _>(slots, destination, value) {
            ScalarStep::Complete | ScalarStep::Released => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }

    fn ready_class_constant(
        &self,
        position: Position,
        class_reg: Reg,
        name: NameId,
    ) -> Option<Value> {
        let function = position.frame.function;
        let name_str = self.code.unit(function).names.get(name.index())?;
        let slot = position.frame.base() + class_reg.index();
        let [Some(target)] = self.registers.window(slot, 1) else {
            return None;
        };
        let target = target.deref();
        if name_str.eq_ignore_ascii_case("class") {
            let Value::Object(object) = &*target else {
                return None;
            };
            return if let Some(linked) = object.class.as_any().downcast_ref::<Class>() {
                Some(linked.name_value.clone())
            } else {
                Some(Value::from(object.class.name()))
            };
        }
        let linked = match &*target {
            Value::Object(object) => object.class.as_any().downcast_ref::<Class>()?,
            Value::String(class_name) => {
                let class_name = std::str::from_utf8(class_name.as_bytes()).ok()?;
                self.classes.get_ref(class_name)?.as_ref()
            }
            _ => return None,
        };
        let prop_key = self.code.unit_prop_key(function, name);
        self.read_linked_constant(position, linked, prop_key)
    }

    fn ready_named_class_constant(
        &self,
        position: Position,
        class: SymbolId,
        name: NameId,
    ) -> Option<Value> {
        let function = position.frame.function;
        let prop_key = self.code.unit_prop_key(function, name);
        let key = self.code.unit_symbol_key(function, class);
        if let Some(linked) = self.classes.by_key(key) {
            return self.read_linked_constant(position, linked, prop_key);
        }
        let written = &self.code.unit(function).symbols.get(class.index())?.written;
        match written.as_str() {
            "self" => {
                let scope = self.caller_scope(position)?;
                self.read_linked_constant(position, scope, prop_key)
            }
            "parent" => {
                let parent = self.caller_scope(position)?.parent.as_deref()?;
                self.read_linked_constant(position, parent, prop_key)
            }
            "static" => {
                let called = self.current_context()?.called()?;
                self.read_linked_constant(position, &called, prop_key)
            }
            _ => None,
        }
    }

    #[inline]
    fn read_linked_constant(
        &self,
        position: Position,
        linked: &Class,
        key: PropKey,
    ) -> Option<Value> {
        if linked.kind == ClassKind::Trait {
            return None;
        }
        let entry = linked.constant_table().get(key)?;
        let constant = &entry.constant;
        if constant.deprecated.is_some()
            || !entry.visible_to(self.caller_scope_index(position), self.classes)
        {
            return None;
        }
        match &constant.initializer {
            ConstantInitializer::EnumCase {
                slot: Some(slot), ..
            } => match self.class_states.constant(*slot) {
                ConstantState::Ready(value) => Some(value.clone()),
                _ => None,
            },
            _ if self.enum_cases.needs_backing(linked) => None,
            ConstantInitializer::Value(value) => Some(value.clone()),
            ConstantInitializer::Compiled { slot, .. } => match self.class_states.constant(*slot) {
                ConstantState::Ready(value) => Some(value.clone()),
                _ => None,
            },
            ConstantInitializer::EnumCase { slot: None, .. } => None,
        }
    }
}
