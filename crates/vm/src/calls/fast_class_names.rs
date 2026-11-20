//! Class names that the interpreter loop reads without the VM.
//!
//! ```php
//! class Repository {
//!     public function name(): string { return static::class; }  // FetchClassName
//! }
//! $type = User::class;                                           // FetchClassName
//! ```
//!
//! `Foo::class` is the name as the file resolved it, a string the file's
//! request-local values already hold; `self::class`, `static::class` and
//! `parent::class` are the name of the running code's class scope, called
//! class or parent class. When there is none (`self::class` outside a
//! class), the VM raises PHP's error: this answers [`Step::Slow`].

use phera_bytecode::{Reg, SymbolId};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::fast::{ScalarStep, store};

impl Machine<'_> {
    /// `FetchClassName`, as `Vm::class_symbol_value` gives it.
    pub(super) fn fetch_class_name<const GUARDED: bool>(
        &mut self,
        position: Position,
        destination: Reg,
        symbol: SymbolId,
    ) -> Step {
        let Some(name) = self.class_name(position, symbol) else {
            return Step::Slow;
        };
        let slots = self.registers.tail_mut(position.frame.base());
        match store::<GUARDED>(slots, destination, name) {
            ScalarStep::Complete => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }

    fn class_name(&self, position: Position, symbol: SymbolId) -> Option<Value> {
        let function = position.frame.function;
        let written = &self
            .code
            .unit(function)
            .symbols
            .get(symbol.index())?
            .written;
        let context = self.current_context();
        let class = match written.as_str() {
            phera_bytecode::TRAIT_CLASS_NAME => self
                .code
                .facts(function)
                .class
                .map(|class| self.classes.class(class))
                .or_else(|| context?.scope())?
                .clone(),
            "self" => context?.scope()?.clone(),
            "static" => context?.called()?,
            "parent" => context?.scope()?.parent.clone()?,
            _ => {
                return self
                    .code
                    .cache(function)
                    .class_names
                    .get(symbol.index())
                    .cloned();
            }
        };
        Some(class.name_value.clone())
    }
}
