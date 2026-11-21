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

use phera_bytecode::{NameId, Reg, SymbolId, Visibility};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::classes::class::{ConstantInitializer, ConstantState};
use crate::fast::{ScalarStep, store};

impl Machine<'_> {
    /// `FetchNamedClassConstant`, when the constant is ready.
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
        match store::<GUARDED>(slots, destination, value) {
            ScalarStep::Complete | ScalarStep::Released => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }

    fn ready_named_class_constant(
        &self,
        position: Position,
        class: SymbolId,
        name: NameId,
    ) -> Option<Value> {
        let function = position.frame.function;
        let key = self.code.unit_symbol_key(function, class);
        let linked = self.classes.by_key(key)?;
        let name = self.code.unit(function).names.get(name.index())?;
        let constant = linked.constants.get(name.as_str())?;
        if constant.deprecated.is_some() || constant.visibility != Visibility::Public {
            return None;
        }
        match &constant.initializer {
            // `Suit::Hearts`, once the request has stored the case object.
            ConstantInitializer::EnumCase {
                slot: Some(slot), ..
            } => match self.class_states.constant(*slot) {
                ConstantState::Ready(value) => Some(value.clone()),
                _ => None,
            },
            // A backed enum checks its cases before any of its constants is
            // used, so its other constants are read out of line, where that
            // check is known. A unit enum has nothing to check.
            _ if linked
                .enum_info
                .as_ref()
                .is_some_and(|info| info.backing.is_some()) =>
            {
                None
            }
            ConstantInitializer::Value(value) => Some(value.clone()),
            ConstantInitializer::Compiled { slot, .. } => match self.class_states.constant(*slot) {
                ConstantState::Ready(value) => Some(value.clone()),
                _ => None,
            },
            ConstantInitializer::EnumCase { slot: None, .. } => None,
        }
    }
}
