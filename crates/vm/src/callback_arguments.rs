//! Callback argument arrays retain names and aliases: `call_user_func_array($f, ['x' => &$x])`.

use std::rc::Rc;

use phera_extension_api::{LocalFuture, NativeError, NativeResult};
use phera_value::{ArrayKey, PhpArray};

use crate::Vm;
use crate::call_state::{Callee, PendingCall};

impl Vm {
    /// Reuse the send protocol's name routing without inventing temporary register operands.
    pub(crate) fn invoke_array_callback(
        &mut self,
        callee: Rc<Callee>,
        array: &PhpArray,
    ) -> LocalFuture<'_, NativeResult> {
        self.invoke_array_callback_origin(
            callee,
            array,
            crate::callback_binding::ArgumentOrigin::Callback,
        )
    }

    pub(crate) fn invoke_array_callback_origin(
        &mut self,
        callee: Rc<Callee>,
        array: &PhpArray,
        origin: crate::callback_binding::ArgumentOrigin,
    ) -> LocalFuture<'_, NativeResult> {
        let result = route_arguments(callee.clone(), array);
        Box::pin(async move {
            let prepared = result?;
            self.invoke_named_callable_origin(
                callee,
                prepared.arguments,
                prepared.named_start,
                prepared.named_keys,
                origin,
            )
            .await
        })
    }
}

/// Names are attached to the variadic collector only after parameter binding has created it.
struct PreparedCallback {
    arguments: Vec<Option<phera_value::Value>>,
    named_start: usize,
    named_keys: Vec<String>,
}

/// A positional element after a named one is an error before the callback executes.
fn route_arguments(callee: Rc<Callee>, array: &PhpArray) -> Result<PreparedCallback, NativeError> {
    let mut call = PendingCall {
        owner: None,
        callee,
        arguments: Vec::new(),
        named: None,
        extra_named: None,
    };
    for (key, value) in array.iter() {
        match key {
            ArrayKey::Int(_) if call.order() == crate::call_state::ArgumentOrder::Named => {
                return Err(NativeError::error(
                    "Cannot use positional argument after named argument",
                ));
            }
            ArrayKey::Int(_) => {}
            ArrayKey::String(name) => call.named = Some(name.to_string_lossy()),
        }
        let index = call.argument_index()?;
        call.store_argument(index, call.named.clone(), value.clone())?;
    }
    let mut named_keys = Vec::new();
    let mut named_start = 0;
    if let Some(extra) = call.extra_named {
        named_start = call
            .arguments
            .len()
            .max(call.callee.parameters.len().saturating_sub(1));
        call.arguments.resize_with(named_start, || None);
        for argument in extra.values {
            named_keys.push(argument.name);
            call.arguments.push(Some(argument.value));
        }
    }
    Ok(PreparedCallback {
        arguments: call.arguments,
        named_start,
        named_keys,
    })
}
