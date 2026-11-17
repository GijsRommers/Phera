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
        let function = frame.function(&self.code);
        let required = self.code.functions[frame.function.0 as usize].required;
        if call.checks == TypeChecks::Complete
            && call.order == crate::calls::call_state::ArgumentOrder::Positional
            && call.supplied >= required
            && function.argument_layout == ArgumentLayout::Fixed
        {
            return Ok(());
        }
        for index in 0..frame.function(&self.code).parameters.len() {
            let parameter = &frame.function(&self.code).parameters[index];
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
        let unit = frame.unit(&self.code).clone();
        let function = frame
            .function_id(&self.code)
            .map_or(&unit.main, |index| &unit.functions[index]);
        let parameter = &function.parameters[index];
        let Some(declaration) = parameter.ty else {
            return Ok(value);
        };
        let scope = frame.cache(&self.code).type_scopes[declaration.index()];
        if types::accepts_as_is(&value, &frame.unit(&self.code).types[declaration.index()]) {
            return Ok(value);
        }
        if scope == types::DeclarationScope::Absolute
            && self.matches_cached_class_type(
                &value,
                declaration,
                frame.unit(&self.code),
                frame.cache(&self.code),
            )
        {
            return Ok(value);
        }
        let scoped = self.call_stack.extras[frame.depth]
            .method
            .as_ref()
            .filter(|_| scope == types::DeclarationScope::Relative)
            .map(|context| {
                crate::closure_types::parameter_type(
                    &frame.unit(&self.code).types[declaration.index()],
                    context.scope,
                    context.called,
                    &self.classes,
                )
            });
        let ty = scoped
            .as_ref()
            .unwrap_or(&frame.unit(&self.code).types[declaration.index()]);
        let name = if parameter.variadic {
            ""
        } else {
            &frame.unit(&self.code).names[parameter.name.index()]
        };
        let ty = ty.clone();
        let name = name.to_owned();
        self.check_argument(value, &ty, &function.name, offset, &name)
    }

    /// Arity errors keep the caller's source location and the complete supplied arity.
    fn missing_argument(&self, frame: &Frame, supplied: usize) -> NativeError {
        let function = frame.function(&self.code);
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
        self.call_stack.extras[frame.depth].supplied_arguments = call.supplied;
        self.call_stack.extras[frame.depth].extra_arguments = call
            .supplied
            .saturating_sub(frame.function(&self.code).parameters.len());
        self.call_stack.extras[frame.depth].argument_count = call.argument_count;
        if !frame.function(&self.code).observes_arguments
            || frame.function(&self.code).argument_layout == ArgumentLayout::Fixed
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
        self.call_stack.extras[frame.depth].saved_arguments = Some(base);
    }
}
