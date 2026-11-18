//! Finish a filled window without moving positional parameters a second time.

use std::rc::Rc;

use phera_bytecode::ArgumentLayout;
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, PhpArray, PhpString, Value};

use crate::calls::call_view::ArgumentSlots;
use crate::calls::frame_extras::ArgumentFacts;
use crate::calls::prepared_call::{CallFlags, PreparedCall};
use crate::code_table::FunctionIndex;
use crate::{Vm, types};

impl Vm {
    /// Missing parameters and coercions are deferred until all argument expressions finish.
    pub(crate) fn bind_window(
        &mut self,
        entry: FunctionIndex,
        call: &PreparedCall,
        slots: ArgumentSlots,
        extra_named: &[(usize, PhpString)],
    ) -> Result<(), NativeError> {
        let facts = self.code.facts(entry);
        let function = self.code.function(entry);
        let deferred = call.flags.contains(CallFlags::DEFERRED_CHECKS);
        if call.is_plain()
            && call.sent >= u32::from(facts.required)
            && function.argument_layout == ArgumentLayout::Fixed
        {
            return Ok(());
        }
        for index in 0..function.parameters.len() {
            let parameter = &self.code.function(entry).parameters[index];
            if parameter.variadic {
                self.collect_variadic((entry, call, slots), index, extra_named)?;
                break;
            }
            let has_default = parameter.has_default;
            let value = self.registers.window(slots.slot(index), 1)[0].clone();
            let Some(value) = value else {
                if has_default {
                    continue;
                }
                return Err(self.missing_argument(entry, call.sent as usize));
            };
            if deferred {
                let value = self.check_window_argument((entry, call), index, index, value)?;
                self.registers.put(slots.base + index, value);
            }
        }
        Ok(())
    }

    /// Variadics consume the parameter slot and surplus region into one PHP array.
    fn collect_variadic(
        &mut self,
        (entry, call, slots): (FunctionIndex, &PreparedCall, ArgumentSlots),
        index: usize,
        extra_named: &[(usize, PhpString)],
    ) -> Result<(), NativeError> {
        let mut values = PhpArray::new();
        let mut failure = None;
        for offset in index..call.sent as usize {
            let value = self.registers.window(slots.slot(offset), 1)[0]
                .clone()
                .unwrap_or_default();
            let checked = if call.flags.contains(CallFlags::DEFERRED_CHECKS) {
                self.check_window_argument((entry, call), index, offset, value.clone())
            } else {
                Ok(value.clone())
            };
            let key = extra_named
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
        self.registers
            .put(slots.base + index, Value::Array(Rc::new(values)));
        failure.map_or(Ok(()), Err)
    }

    /// Relative class types use the callee's lexical scope while strictness comes from the caller.
    pub(super) fn check_window_argument(
        &mut self,
        (entry, call): (FunctionIndex, &PreparedCall),
        index: usize,
        offset: usize,
        value: Value,
    ) -> Result<Value, NativeError> {
        let parameter = &self.code.function(entry).parameters[index];
        let Some(declaration) = parameter.ty else {
            return Ok(value);
        };
        let unit = self.code.unit(entry);
        let cache = self.code.cache(entry);
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
        let name = match parameter.variadic {
            true => String::new(),
            false => unit.names[parameter.name.index()].clone(),
        };
        let function_name = self.code.function_name(entry);
        let function_name = function_name
            .split('\0')
            .next()
            .unwrap_or(function_name)
            .to_owned();
        let declared = &unit.types[declaration.index()];
        let context = match scope {
            types::DeclarationScope::Relative => self.prepared_context(entry, call).method(),
            _ => None,
        };
        let scoped = context
            .as_ref()
            .map(|context| crate::closure_types::parameter_type(declared, context));
        let ty = scoped.as_ref().unwrap_or(declared);
        self.check_argument(value, ty, &function_name, offset, &name)
    }

    /// Arity errors keep the caller's source location and the complete supplied arity.
    pub(crate) fn missing_argument(&self, function: FunctionIndex, supplied: usize) -> NativeError {
        let name = self.code.function_name(function);
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
            name, location.file, location.line,
        ))
    }

    /// How the call passed its arguments, when the frame must remember it:
    /// fewer or more arguments than parameters, named variadic entries, or a
    /// variadic function that reads `func_get_args()`, which gets a copy of
    /// the original arguments above the window.
    pub(crate) fn save_window_arguments(
        &mut self,
        function: FunctionIndex,
        call: &PreparedCall,
        slots: ArgumentSlots,
    ) -> Option<ArgumentFacts> {
        let body = self.code.function(function);
        let parameters = body.parameters.len();
        let (supplied, counted) = (call.sent as usize, call.counted as usize);
        let mut facts = ArgumentFacts {
            supplied: (supplied != parameters).then_some(supplied),
            count: (counted != parameters).then_some(counted),
            extra: supplied.saturating_sub(parameters),
            saved: None,
        };
        if body.observes_arguments && body.argument_layout != ArgumentLayout::Fixed {
            let base = self.registers.len();
            self.registers.grow_to(base + counted);
            for index in 0..counted {
                let value = self.registers.window(slots.slot(index), 1)[0].clone();
                self.registers.window_mut(base + index, 1)[0] = value;
            }
            facts.saved = Some(base);
        }
        let needed = facts.supplied.is_some() || facts.count.is_some() || facts.saved.is_some();
        needed.then_some(facts)
    }
}
