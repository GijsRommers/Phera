//! Finalize a filled window without moving positional parameters a second time.

use std::rc::Rc;

use phera_bytecode::{ArgumentLayout, Reg};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, PhpArray, Value};

use crate::calls::call_state::{CallWindow, TypeChecks};
use crate::calls::frame::Frame;
use crate::{Vm, types};

impl Vm {
    /// Missing parameters and coercions are deferred until all argument expressions finish.
    pub(crate) fn bind_window(
        &mut self,
        frame: &mut Frame,
        call: &CallWindow,
    ) -> Result<(), NativeError> {
        let function = frame.function();
        let required = frame
            .function_id
            .and_then(|index| frame.cache.required_parameters.get(index))
            .copied()
            .unwrap_or(0);
        if call.checks == TypeChecks::Complete
            && call.order == crate::calls::call_state::ArgumentOrder::Positional
            && call.supplied >= required
            && function.argument_layout == ArgumentLayout::Fixed
        {
            return Ok(());
        }
        for index in 0..frame.function().parameters.len() {
            let parameter = &frame.function().parameters[index];
            if parameter.variadic {
                self.collect_variadic(frame, call, index)?;
                break;
            }
            let value = self.registers.window(call.slot(index), 1)[0].clone();
            let Some(value) = value else {
                if parameter.has_default {
                    continue;
                }
                return Err(self.missing_argument(frame, call.supplied));
            };
            if call.checks == TypeChecks::Deferred {
                let value = self.check_window_argument(frame, index, index, value)?;
                frame.set(&mut self.registers, Reg(index as u16), value);
            }
        }
        Ok(())
    }

    /// Variadics consume the parameter slot and surplus region into one PHP array.
    fn collect_variadic(
        &mut self,
        frame: &mut Frame,
        call: &CallWindow,
        index: usize,
    ) -> Result<(), NativeError> {
        let mut values = PhpArray::new();
        for offset in index..call.supplied {
            let value = self.registers.window(call.slot(offset), 1)[0]
                .clone()
                .unwrap_or_default();
            let checked = if call.checks == TypeChecks::Deferred {
                self.check_window_argument(frame, index, offset, value.clone())
            } else {
                Ok(value.clone())
            };
            let key = call
                .extra_named
                .iter()
                .find(|(slot, _)| *slot == offset)
                .map(|(_, name)| ArrayKey::String(name.clone()));
            match key {
                Some(key) => {
                    values.insert(key, checked.as_ref().unwrap_or(&value).clone());
                }
                None => {
                    values.push(checked.as_ref().unwrap_or(&value).clone())?;
                }
            }
            if let Err(error) = checked {
                frame.set(
                    &mut self.registers,
                    Reg(index as u16),
                    Value::Array(Rc::new(values)),
                );
                return Err(error);
            }
        }
        frame.set(
            &mut self.registers,
            Reg(index as u16),
            Value::Array(Rc::new(values)),
        );
        Ok(())
    }

    /// Relative class types use the callee's lexical scope while strictness comes from the caller.
    fn check_window_argument(
        &mut self,
        frame: &Frame,
        index: usize,
        offset: usize,
        value: Value,
    ) -> Result<Value, NativeError> {
        let function = frame.function();
        let parameter = &function.parameters[index];
        let Some(declaration) = parameter.ty else {
            return Ok(value);
        };
        let scope = frame.cache.type_scopes[declaration.index()];
        if types::accepts_as_is(&value, &frame.unit.types[declaration.index()]) {
            return Ok(value);
        }
        if scope == types::DeclarationScope::Absolute
            && self.matches_cached_class_type(&value, declaration, &frame.unit, &frame.cache)
        {
            return Ok(value);
        }
        let scoped = frame
            .method
            .as_ref()
            .filter(|_| scope == types::DeclarationScope::Relative)
            .map(|context| {
                crate::closure_types::parameter_type(
                    &frame.unit.types[declaration.index()],
                    context,
                )
            });
        let ty = scoped
            .as_ref()
            .unwrap_or(&frame.unit.types[declaration.index()]);
        let name = if parameter.variadic {
            ""
        } else {
            &frame.unit.names[parameter.name.index()]
        };
        self.check_argument(value, ty, &function.name, offset, name)
    }

    /// Arity errors keep the caller's source location and the complete supplied arity.
    fn missing_argument(&self, frame: &Frame, supplied: usize) -> NativeError {
        let function = frame.function();
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

    /// Observable argument snapshots occupy a tail of the same window; ordinary calls skip it.
    pub(crate) fn save_window_arguments(&mut self, frame: &mut Frame, call: &CallWindow) {
        frame.supplied_arguments = call.supplied;
        frame.extra_arguments = call
            .supplied
            .saturating_sub(frame.function().parameters.len());
        frame.argument_count = call.argument_count;
        if !frame.function().observes_arguments
            || frame.function().argument_layout == ArgumentLayout::Fixed
        {
            return;
        }
        let count = call.argument_count;
        let base = self.registers.len();
        self.registers.grow_to(base + count);
        for index in 0..count {
            let value = self.registers.window(call.slot(index), 1)[0].clone();
            self.registers.window_mut(base + index, 1)[0] = value;
        }
        frame.saved_arguments = Some(base);
    }
}
