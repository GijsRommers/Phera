//! Parameter binding keeps plain supplied arguments separate from PHP coercion.

use std::rc::Rc;

use phera_bytecode::{CompiledUnit, Reg};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{PhpArray, Value};

use crate::{Vm, types};

impl Vm {
    /// Bind supplied parameters in declaration order; default bytecode handles missing defaults.
    pub(crate) fn bind_parameters(
        &mut self,
        frame: &mut crate::frame::Frame,
        arguments: &mut [Option<Value>],
        function_id: usize,
    ) -> Result<(), NativeError> {
        if self.bind_plain_parameters(frame, arguments, function_id) {
            return Ok(());
        }
        let code = frame.unit.clone();
        let function = code
            .functions
            .get(function_id)
            .ok_or_else(|| internal_error!("call function"))?;
        let parameters = &function.parameters;
        for (i, parameter) in parameters.iter().enumerate() {
            if arguments.get(i).is_none_or(Option::is_none) && parameter.has_default {
                continue;
            }
            let value = if parameter.variadic {
                match self.variadic_arguments(
                    &code,
                    function,
                    parameter,
                    arguments,
                    i,
                    frame.method.as_deref(),
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        let mut original = PhpArray::new();
                        for argument in arguments.iter().skip(i) {
                            original.push(argument.clone().unwrap_or_default())?;
                        }
                        frame.set(
                            &mut self.registers,
                            Reg(i as u16),
                            Value::Array(Rc::new(original)),
                        );
                        return Err(error);
                    }
                }
            } else {
                let value = arguments.get_mut(i).and_then(Option::take).ok_or_else(|| {
                    let required = parameters.iter().filter(|parameter| !parameter.has_default && !parameter.variadic).count();
                    let location = self.location();
                    let qualifier = if required == parameters.len() { "exactly" } else { "at least" };
                    NativeError::argument_count_error(format!(
                        "Too few arguments to function {}(), {} passed in {} on line {} and {qualifier} {required} expected",
                        function.name, arguments.len(), location.file, location.line,
                    ))
                })?;
                if parameter.ty.is_none() {
                    value
                } else {
                    let scope = parameter
                        .ty
                        .and_then(|index| frame.cache.type_scopes.get(index.index()).copied())
                        .unwrap_or(types::DeclarationScope::Relative);
                    if matches!(&value, Value::Object(_))
                        && scope == types::DeclarationScope::Absolute
                        && parameter.ty.is_some_and(|declaration| {
                            self.matches_cached_class_type(&value, declaration, &code, &frame.cache)
                        })
                    {
                        frame.set(&mut self.registers, Reg(i as u16), value);
                        continue;
                    }
                    let original = value.clone();
                    let scoped_type = parameter
                        .ty
                        .filter(|_| scope == types::DeclarationScope::Relative)
                        .and_then(|ty| {
                            frame.method.as_ref().map(|context| {
                                crate::closure_types::parameter_type(
                                    &code.types[ty.index()],
                                    context,
                                )
                            })
                        });
                    let declared_type = scoped_type
                        .as_ref()
                        .or_else(|| parameter.ty.map(|ty| &code.types[ty.index()]));
                    match self.check_argument(
                        value,
                        declared_type,
                        scope,
                        &function.name,
                        i,
                        &code.names[parameter.name.index()],
                    ) {
                        Ok(value) => value,
                        Err(error) => {
                            frame.set(&mut self.registers, Reg(i as u16), original);
                            return Err(error);
                        }
                    }
                }
            };
            frame.set(&mut self.registers, Reg(i as u16), value);
        }
        Ok(())
    }
}

impl Vm {
    /// Mirrors the untyped, nonvariadic arm of bind_parameters. All required
    /// arguments must exist; missing/default, typed and variadic cases use the
    /// full binder. Moving raw values preserves reference parameters unchanged.
    fn bind_plain_parameters(
        &mut self,
        frame: &crate::frame::Frame,
        arguments: &mut [Option<Value>],
        function: usize,
    ) -> bool {
        let Some(count) = frame
            .cache
            .plain_parameters
            .get(function)
            .copied()
            .flatten()
        else {
            return false;
        };
        let Some(arguments) = arguments
            .get_mut(..count)
            .filter(|values| values.iter().all(Option::is_some))
        else {
            return false;
        };
        for (slot, argument) in self
            .registers
            .tail_mut(frame.base)
            .iter_mut()
            .zip(arguments)
        {
            *slot = argument.take();
        }
        true
    }
}

impl Vm {
    /// A variadic parameter collects remaining values in order and checks each element.
    fn variadic_arguments(
        &mut self,
        code: &CompiledUnit,
        function: &phera_bytecode::Function,
        parameter: &phera_bytecode::Parameter,
        arguments: &[Option<Value>],
        index: usize,
        context: Option<&crate::classes::MethodContext>,
    ) -> NativeResult {
        let mut values = PhpArray::new();
        let scope = parameter
            .ty
            .map(|index| types::declaration_scope(&code.types[index.index()]))
            .unwrap_or(types::DeclarationScope::Absolute);
        let declared_type = parameter.ty.map(|ty| &code.types[ty.index()]);
        let scoped_type = declared_type.and_then(|ty| {
            context
                .filter(|_| scope == types::DeclarationScope::Relative)
                .map(|context| crate::closure_types::parameter_type(ty, context))
        });
        let declared_type = scoped_type.as_ref().or(declared_type);
        for (offset, argument) in arguments.iter().enumerate().skip(index) {
            let value = argument.clone().unwrap_or_default();
            let value =
                self.check_argument(value, declared_type, scope, &function.name, offset, "")?;
            values.push(value)?;
        }
        Ok(Value::Array(Rc::new(values)))
    }
}
