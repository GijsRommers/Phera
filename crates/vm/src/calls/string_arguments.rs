//! The weak call `accept_string($object)` casts the object after argument
//! evaluation and before the callee starts, using the ordinary call window.

use phera_extension_api::{NativeError, Param, ParamType};
use phera_value::Value;

use crate::Vm;
use crate::calls::call_view::CallView;
use crate::calls::callback_binding::ArgumentOrigin;
use crate::calls::prepared_call::{CallFlags, PreparedCallee};

impl Vm {
    #[cold]
    pub(super) fn deferred_string_coercion(&self, origin: ArgumentOrigin) -> bool {
        let Ok(view) = CallView::top(&self.code, &self.call_stack) else {
            return false;
        };
        if self.call_is_strict(origin, view.call.flags) {
            return false;
        }
        let parameters = view.parameters();
        let slots = view.slots();
        (0..view.call.sent as usize).any(|index| {
            let ty = parameters.receiving(index).and_then(|parameter| parameters.ty(parameter));
            let value = self.registers.window(slots.slot(index), 1)[0].as_ref();
            matches!((ty, value), (Some(ty), Some(value)) if self.needs_string_coercion(value, ty, crate::types::TypeChecking::Coerce))
        })
    }

    pub(crate) fn coerce_call_strings(
        &mut self,
        origin: ArgumentOrigin,
    ) -> phera_extension_api::LocalFuture<'_, Result<(), NativeError>> {
        Box::pin(self.convert_window_arguments(origin))
    }

    async fn convert_window_arguments(
        &mut self,
        origin: ArgumentOrigin,
    ) -> Result<(), NativeError> {
        let view = CallView::top(&self.code, &self.call_stack)?;
        let call = view.call;
        let PreparedCallee::Php(function) = call.callee else {
            return Ok(());
        };
        let slots = view.slots();
        let count = call.sent as usize;
        let origin = if call.flags.contains(CallFlags::INVOKE) {
            ArgumentOrigin::Callback
        } else {
            origin
        };
        self.call_stack
            .top_call_mut()?
            .flags
            .remove(CallFlags::STRING_ARGUMENT);
        for index in 0..count {
            let parameters = CallView::top(&self.code, &self.call_stack)?.parameters();
            let Some(parameter) = parameters.receiving(index) else {
                break;
            };
            let slot = slots.slot(index);
            let Some(value) = self.registers.window(slot, 1)[0].clone() else {
                if !self.code.function(function).parameters[parameter].has_default {
                    return Ok(());
                }
                continue;
            };
            let convertible = parameters.ty(parameter).is_some_and(|ty| {
                self.needs_string_coercion(&value, ty, crate::types::TypeChecking::Coerce)
            });
            let value = if convertible {
                self.coerce_stringable(value).await?
            } else {
                match self.with_argument_origin(origin, |vm| {
                    vm.check_window_argument((function, &call), parameter, index, value)
                }) {
                    Ok(value) => value,
                    // Normal entry supplies the frame and trace for a parameter error.
                    Err(_) => return Ok(()),
                }
            };
            self.registers.put(slot, value);
        }
        Ok(())
    }

    pub(crate) fn call_is_strict(&self, origin: ArgumentOrigin, flags: CallFlags) -> bool {
        origin == ArgumentOrigin::Php
            && !flags.contains(CallFlags::INVOKE)
            && self
                .current()
                .is_some_and(|frame| frame.function().strict_types)
    }

    /// Check the value kind before even examining the declared native parameter.
    #[inline]
    pub(crate) fn native_strings_needed(
        &self,
        parameters: &[Param],
        arguments: &[Value],
        origin: ArgumentOrigin,
    ) -> bool {
        arguments.iter().enumerate().any(|(index, value)| {
            value.is_object()
                && native_string_param(parameters, index, value)
                && Self::has_string_conversion(value)
        }) && !self.call_is_strict(origin, CallFlags::default())
    }

    #[cold]
    pub(crate) async fn coerce_native_strings(
        &mut self,
        name: &str,
        parameters: &[Param],
        arguments: &mut [Value],
        origin: ArgumentOrigin,
    ) -> Result<(), NativeError> {
        let required = parameters
            .iter()
            .filter(|parameter| !parameter.optional)
            .count();
        if arguments.len() < required
            || (arguments.len() > parameters.len()
                && !parameters.last().is_some_and(|p| p.variadic))
        {
            return Ok(());
        }
        for (index, argument) in arguments.iter_mut().enumerate() {
            if native_string_param(parameters, index, argument)
                && Self::has_string_conversion(argument)
            {
                *argument = self.coerce_stringable(argument.clone()).await?;
            } else {
                let Some(param) = parameters.get(index).or(parameters.last()) else {
                    break;
                };
                self.with_argument_origin(origin, |vm| {
                    phera_extension_api::prepare_single_argument(name, index, param, argument, vm)
                })?;
            }
        }
        Ok(())
    }
}

fn native_string_param(parameters: &[Param], index: usize, value: &Value) -> bool {
    parameters
        .get(index)
        .or(parameters.last())
        .is_some_and(|parameter| {
            if let ParamType::ClassUnion(_) = parameter.ty {
                return parameter.ty.union_accepts_string()
                    && matches!(&*value.deref(), Value::Object(object)
                        if !parameter.ty.union_accepts_class(object.class.as_ref()));
            }
            matches!(
                parameter.ty,
                ParamType::String
                    | ParamType::ArrayOrString
                    | ParamType::IntOrString
                    | ParamType::StringOrNumber
                    | ParamType::CertificateOrString
            )
        })
}
