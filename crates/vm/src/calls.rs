//! Call preparation, signature handling and VM intrinsics.

use std::{rc::Rc, sync::Arc};

use phera_bytecode::{CompiledUnit, Reg};
use phera_extension_api::{Context, NativeError, NativeFn, NativeResult, prepare_arguments};
use phera_value::{PhpArray, Value};

use crate::{Vm, intrinsics::Intrinsic, types};

impl Vm {
    /// Dynamic-scope operations reuse declared locals before allocating a new named slot.
    pub(crate) fn named_slot(&mut self, name: &str) -> Result<Reg, NativeError> {
        let frame = self
            .call_stack
            .current_mut()
            .ok_or_else(|| internal_error!("no variable frame"))?;
        if let Some(i) = frame.function().locals.iter().position(|candidate| {
            frame
                .unit
                .names
                .get(candidate.index())
                .is_some_and(|value| value == name)
        }) {
            return Ok(Reg(i as u16));
        }
        if let Some(register) = frame.dynamic.get(name) {
            return Ok(*register);
        }
        let register = Reg(u16::try_from(frame.count)
            .map_err(|_| NativeError::error("Phera register limit exceeded"))?);
        self.registers.push_undefined();
        frame.count += 1;
        frame.dynamic.insert(name.into(), register);
        Ok(register)
    }

    pub(crate) fn exists(&self, name: &str) -> bool {
        self.function_table.contains(&name.to_ascii_lowercase())
            || self.registry.get_function(name).is_some()
            || Intrinsic::resolve(name).is_some()
    }

    /// Convert callback values to the same optional-argument representation as PHP calls.
    pub(crate) fn push_call(
        &mut self,
        unit: Arc<CompiledUnit>,
        function_id: usize,
        destination: Option<Reg>,
        arguments: Vec<Value>,
    ) -> Result<(), NativeError> {
        self.push_prepared(
            unit,
            function_id,
            destination,
            arguments.into_iter().map(Some).collect(),
        )
    }

    /// Bound PHP recursion by heap frame count, then preserve observable arguments.
    pub(crate) fn push_prepared(
        &mut self,
        unit: Arc<CompiledUnit>,
        function_id: usize,
        destination: Option<Reg>,
        mut arguments: Vec<Option<Value>>,
    ) -> Result<(), NativeError> {
        if self.call_stack.depth() >= self.frame_limit {
            return Err(NativeError::error(format!(
                "Maximum call stack depth of {} exceeded",
                self.frame_limit
            )));
        }
        let mut frame = self.make_frame(unit, Some(function_id), destination);
        if frame.function().observes_arguments {
            frame.arguments = arguments
                .iter()
                .cloned()
                .map(Option::unwrap_or_default)
                .collect();
        }
        if let Err(error) = self.bind_parameters(&mut frame, &mut arguments, function_id) {
            self.call_stack.push(frame);
            return Err(error);
        }
        arguments.clear();
        self.call_stack.recycle_arguments(arguments);
        self.call_stack.push(frame);
        Ok(())
    }

    /// Coercion of a reference updates its cell so the caller observes it.
    fn check_argument(
        &mut self,
        value: Value,
        declaration_type: Option<&phera_bytecode::Type>,
        function: &str,
        index: usize,
        parameter: &str,
    ) -> NativeResult {
        let Some(declaration_type) = declaration_type else {
            return Ok(value);
        };
        let crate::diagnostics::SourceLocation { file, line } = self.location();
        let result = types::coerce(value.deref().clone(),declaration_type,self.strict_types().into(),self)
            .map_err(|error| match error {
                NativeError::Throw { class,message } => {
                    let number = index+1;
                    let message = format!("{function}(): Argument #{number} (${parameter}) {message}, called in {file} on line {line}");
                    NativeError::throw(class,message)
                },
                other => other,
            })?;
        if let Value::Reference(reference) = value {
            *reference
                .try_borrow_mut()
                .map_err(|_| internal_error!("argument reference"))? = result;
            Ok(Value::Reference(reference))
        } else {
            Ok(result)
        }
    }

    /// Resolve VM intrinsics before looking up extension handlers.
    pub(crate) async fn native(&mut self, name: &str, arguments: Vec<Value>) -> NativeResult {
        if let Some(intrinsic) = Intrinsic::resolve(name) {
            return self.intrinsic(intrinsic, arguments).await;
        }
        let entry =
            self.registry.get_function(name).copied().ok_or_else(|| {
                NativeError::error(format!("Call to undefined function {name}()"))
            })?;
        self.native_entry(entry, arguments).await
    }

    /// Validate native arguments before executing or awaiting the extension handler.
    pub(crate) async fn native_entry(
        &mut self,
        entry: phera_extension_api::FunctionEntry,
        mut arguments: Vec<Value>,
    ) -> NativeResult {
        if let Err(error) = prepare_arguments(&entry, &mut arguments, self) {
            if !matches!(
                entry.name,
                "strlen" | "count" | "sizeof" | "array_key_exists"
            ) {
                self.failed_native = Some(crate::error_traces::NativeTrace::new(
                    entry.name, &arguments,
                ));
            }
            return Err(error);
        }
        match entry.handler {
            NativeFn::Sync(handler) => {
                let result = handler(self, &arguments);
                if result.is_err()
                    && !matches!(
                        entry.name,
                        "strlen" | "count" | "sizeof" | "array_key_exists"
                    )
                {
                    self.failed_native = Some(crate::error_traces::NativeTrace::new(
                        entry.name, &arguments,
                    ));
                }
                result
            }
            NativeFn::Async(frame) => frame(self, arguments).await,
        }
    }
}

impl Vm {
    /// Bind supplied parameters in declaration order; default bytecode handles missing defaults.
    fn bind_parameters(
        &mut self,
        frame: &mut crate::frame::Frame,
        arguments: &mut [Option<Value>],
        function_id: usize,
    ) -> Result<(), NativeError> {
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
                self.variadic_arguments(&code, function, parameter, arguments, i)?
            } else {
                let value = arguments.get_mut(i).and_then(Option::take).ok_or_else(|| {
                    let required = parameters.iter().filter(|parameter| !parameter.has_default && !parameter.variadic).count();
                    NativeError::argument_count_error(format!(
                        "Too few arguments to function {}(), {} passed and at least {required} expected",
                        function.name,arguments.len(),
                    ))
                })?;
                if parameter.ty.is_none() {
                    value
                } else {
                    let original = value.clone();
                    match self.check_argument(
                        value,
                        parameter
                            .ty
                            .map(|declaration_type| &code.types[declaration_type.index()]),
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
    /// A variadic parameter collects remaining values in order and checks each element.
    fn variadic_arguments(
        &mut self,
        code: &CompiledUnit,
        function: &phera_bytecode::Function,
        parameter: &phera_bytecode::Parameter,
        arguments: &[Option<Value>],
        index: usize,
    ) -> NativeResult {
        let mut values = PhpArray::new();
        for (offset, argument) in arguments.iter().enumerate().skip(index) {
            let value = argument.clone().unwrap_or_default();
            let value = self.check_argument(
                value,
                parameter
                    .ty
                    .map(|declaration_type| &code.types[declaration_type.index()]),
                &function.name,
                offset,
                &code.names[parameter.name.index()],
            )?;
            values.push(value)?;
        }
        Ok(Value::Array(Rc::new(values)))
    }
}
