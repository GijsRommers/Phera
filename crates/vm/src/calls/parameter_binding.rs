//! Finish a filled window without moving positional parameters a second time.

use std::rc::Rc;

use phera_bytecode::ArgumentLayout;
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, PhpArray, Value};

use crate::calls::call_execution::Entry;
use crate::calls::call_state::{ArgumentOrder, CallWindow, TypeChecks};
use crate::calls::frame_extras::ArgumentFacts;
use crate::code_table::FunctionIndex;
use crate::{Vm, types};

impl Vm {
    /// Missing parameters and coercions are deferred until all argument expressions finish.
    pub(crate) fn bind_window(
        &mut self,
        entry: &Entry,
        call: &CallWindow,
    ) -> Result<(), NativeError> {
        let facts = self.code.facts(entry.function);
        let function = self.code.function(entry.function);
        if call.checks == TypeChecks::Complete
            && call.order == ArgumentOrder::Positional
            && call.supplied >= facts.required as usize
            && function.argument_layout == ArgumentLayout::Fixed
        {
            return Ok(());
        }
        for index in 0..function.parameters.len() {
            let parameter = &self.code.function(entry.function).parameters[index];
            if parameter.variadic {
                self.collect_variadic(entry, call, index)?;
                break;
            }
            let has_default = parameter.has_default;
            let value = self.registers.window(call.slot(index), 1)[0].clone();
            let Some(value) = value else {
                if has_default {
                    continue;
                }
                return Err(self.missing_argument(entry.function, call.supplied));
            };
            if call.checks == TypeChecks::Deferred {
                let value = self.check_window_argument(entry, index, index, value)?;
                self.registers.window_mut(call.base + index, 1)[0] = Some(value);
            }
        }
        Ok(())
    }

    /// Variadics consume the parameter slot and surplus region into one PHP array.
    fn collect_variadic(
        &mut self,
        entry: &Entry,
        call: &CallWindow,
        index: usize,
    ) -> Result<(), NativeError> {
        let mut values = PhpArray::new();
        let mut failure = None;
        for offset in index..call.supplied {
            let value = self.registers.window(call.slot(offset), 1)[0]
                .clone()
                .unwrap_or_default();
            let checked = if call.checks == TypeChecks::Deferred {
                self.check_window_argument(entry, index, offset, value.clone())
            } else {
                Ok(value.clone())
            };
            let key = call
                .extra_named
                .iter()
                .find(|(slot, _)| *slot == offset)
                .map(|(_, name)| ArrayKey::String(name.clone()));
            let stored = checked.as_ref().unwrap_or(&value).clone();
            match key {
                Some(key) => {
                    values.insert(key, stored);
                }
                None => {
                    values.push(stored)?;
                }
            }
            if let Err(error) = checked {
                failure = Some(error);
                break;
            }
        }
        self.registers.window_mut(call.base + index, 1)[0] = Some(Value::Array(Rc::new(values)));
        failure.map_or(Ok(()), Err)
    }

    /// Relative class types use the callee's lexical scope while strictness comes from the caller.
    fn check_window_argument(
        &mut self,
        entry: &Entry,
        index: usize,
        offset: usize,
        value: Value,
    ) -> Result<Value, NativeError> {
        let parameter = &self.code.function(entry.function).parameters[index];
        let Some(declaration) = parameter.ty else {
            return Ok(value);
        };
        let unit = self.code.unit(entry.function);
        let cache = self.code.cache(entry.function);
        let scope = cache.type_scopes[declaration.index()];
        if types::accepts_as_is(&value, &unit.types[declaration.index()]) {
            return Ok(value);
        }
        if scope == types::DeclarationScope::Absolute
            && self.matches_cached_class_type(&value, declaration, unit, cache)
        {
            return Ok(value);
        }
        // The coercion below may run PHP code (a `__toString`), so it gets
        // its own owner of the unit instead of a borrow of the code table.
        let unit = unit.clone();
        let function = self.code.function(entry.function);
        let name = match parameter.variadic {
            true => String::new(),
            false => unit.names[parameter.name.index()].clone(),
        };
        let function_name = function.name.clone();
        let declared = &unit.types[declaration.index()];
        let scoped = entry
            .method
            .as_ref()
            .filter(|_| scope == types::DeclarationScope::Relative)
            .map(|context| crate::closure_types::parameter_type(declared, context));
        let ty = scoped.as_ref().unwrap_or(declared);
        self.check_argument(value, ty, &function_name, offset, &name)
    }

    /// Arity errors keep the caller's source location and the complete supplied arity.
    pub(crate) fn missing_argument(&self, function: FunctionIndex, supplied: usize) -> NativeError {
        let function = self.code.function(function);
        let required = function
            .parameters
            .iter()
            .filter(|parameter| !parameter.has_default && !parameter.variadic)
            .count();
        let qualifier = if required == function.parameters.len() {
            "exactly"
        } else {
            "at least"
        };
        let location = self.location();
        NativeError::argument_count_error(format!(
            "Too few arguments to function {}(), {supplied} passed in {} on line {} and \
             {qualifier} {required} expected",
            function.name, location.file, location.line,
        ))
    }

    /// How the call passed its arguments, when the frame must remember it:
    /// fewer or more arguments than parameters, named variadic entries, or a
    /// variadic function that reads `func_get_args()`, which gets a copy of
    /// the original arguments above the window.
    pub(crate) fn save_window_arguments(
        &mut self,
        function: FunctionIndex,
        call: &CallWindow,
    ) -> Option<ArgumentFacts> {
        let body = self.code.function(function);
        let parameters = body.parameters.len();
        let mut facts = ArgumentFacts {
            supplied: (call.supplied != parameters).then_some(call.supplied),
            count: (call.argument_count != parameters).then_some(call.argument_count),
            extra: call.supplied.saturating_sub(parameters),
            saved: None,
        };
        if body.observes_arguments && body.argument_layout != ArgumentLayout::Fixed {
            let count = call.argument_count;
            let base = self.registers.len();
            self.registers.grow_to(base + count);
            for index in 0..count {
                let value = self.registers.window(call.slot(index), 1)[0].clone();
                self.registers.window_mut(base + index, 1)[0] = value;
            }
            facts.saved = Some(base);
        }
        let needed = facts.supplied.is_some() || facts.count.is_some() || facts.saved.is_some();
        needed.then_some(facts)
    }
}
