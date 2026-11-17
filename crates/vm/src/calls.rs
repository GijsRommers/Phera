//! Call preparation, signature handling and VM intrinsics.

use std::sync::Arc;

use phera_bytecode::{CompiledUnit, Reg};
use phera_extension_api::{Context, NativeError, NativeFn, NativeResult};
use phera_value::Value;

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
            &unit,
            function_id,
            destination,
            arguments.into_iter().map(Some).collect(),
        )
    }

    /// Bound PHP recursion by heap frame count, then preserve observable arguments.
    pub(crate) fn push_prepared(
        &mut self,
        unit: &Arc<CompiledUnit>,
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
        frame.supplied_arguments = arguments.len();
        if frame.function().observes_arguments
            || (arguments.len() > frame.function().parameters.len()
                && !frame
                    .function()
                    .parameters
                    .last()
                    .is_some_and(|parameter| parameter.variadic))
        {
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
    #[inline]
    pub(crate) fn check_argument(
        &mut self,
        value: Value,
        declaration_type: Option<&phera_bytecode::Type>,
        scope: types::DeclarationScope,
        function: &str,
        index: usize,
        parameter: &str,
    ) -> NativeResult {
        let Some(declaration_type) = declaration_type else {
            return Ok(value);
        };
        self.check_typed_argument(value, declaration_type, scope, function, index, parameter)
    }

    /// Keep class-type coercion off the ordinary untyped call's parameter-binding path.
    fn check_typed_argument(
        &mut self,
        value: Value,
        declaration_type: &phera_bytecode::Type,
        scope: types::DeclarationScope,
        function: &str,
        index: usize,
        parameter: &str,
    ) -> NativeResult {
        // Mirrors the class arm of types::coerce without cloning declarations or
        // formatting a source location for successful non-relative class types.
        if value.deref().is_object()
            && scope == types::DeclarationScope::Absolute
            && self.matches_class_type(&value, declaration_type)
        {
            return Ok(value);
        }
        let crate::diagnostics::SourceLocation { file, line } = self.location();
        let mut resolved_type = declaration_type.clone();
        if let Some((scope, _)) = function.split_once("::") {
            let parent = self
                .classes
                .get(scope)
                .and_then(|class| class.parent.clone());
            resolved_type.display = declaration_type
                .display
                .split_inclusive(['|', '&', '?'])
                .map(|part| {
                    let separator = part
                        .chars()
                        .last()
                        .filter(|character| matches!(character, '|' | '&' | '?'));
                    let name = part.trim_end_matches(['|', '&', '?']);
                    let name = match name {
                        "self" => scope,
                        "parent" => parent.as_ref().map_or(name, |parent| parent.name.as_str()),
                        _ => name,
                    };
                    format!(
                        "{name}{}",
                        separator
                            .map(|separator| separator.to_string())
                            .unwrap_or_default()
                    )
                })
                .collect();
        }
        let result = types::coerce(value.deref().clone(),&resolved_type,self.strict_types().into(),self)
            .map_err(|error| match error {
                NativeError::Throw { class, message, .. } => {
                    let number = index+1;
                    let label = if parameter.is_empty() { String::new() } else { format!(" (${parameter})") };
                    let message = format!("{function}(): Argument #{number}{label} {message}, called in {file} on line {line}");
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
        arguments: Vec<Value>,
    ) -> NativeResult {
        if self.error_handlers.error.is_some() && !self.error_handlers.active {
            self.native_entry_traced(entry, arguments).await
        } else {
            self.native_entry_body(entry, arguments).await
        }
    }

    /// Handler-only state uses an erased future so ordinary native calls retain their small frame.
    fn native_entry_traced(
        &mut self,
        entry: phera_extension_api::FunctionEntry,
        arguments: Vec<Value>,
    ) -> phera_extension_api::LocalFuture<'_, NativeResult> {
        Box::pin(async move {
            let previous =
                self.error_handlers
                    .native_site
                    .replace(crate::error_traces::NativeTrace::new(
                        entry.name, &arguments,
                    ));
            let result = self.native_entry_body(entry, arguments).await;
            self.error_handlers.native_site = previous;
            result
        })
    }

    /// Keep warning trace capture outside native execution when no handler is installed.
    async fn native_entry_body(
        &mut self,
        entry: phera_extension_api::FunctionEntry,
        mut arguments: Vec<Value>,
    ) -> NativeResult {
        if let Err(error) = self.prepare_call_with_trace(entry.name, entry.params, &mut arguments) {
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
